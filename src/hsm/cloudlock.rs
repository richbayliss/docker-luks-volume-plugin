use crate::crypto::*;
use base64;
use openssl::rand::rand_bytes;
use serde::{Deserialize, Serialize};
use url;

#[derive(Serialize, Deserialize, PartialEq)]
pub struct CloudLockPayload {
    pub data: String,
}

pub struct CloudLockHSM {
    api_key: String,
    base_url: url::Url,
    cert: openssl::x509::X509,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct CloudLockConfig {
    #[serde(rename = "cert")]
    pub cert_pem: String,
}

impl CloudLockHSM {
    pub fn new(
        uuid: &str,
        api_key: &str,
        api_host: &str,
        api_version: &str,
    ) -> Result<Self, String> {
        let base_url = url::Url::parse(&format!(
            "https://{host}/cloudlock/{version}/{uuid}/",
            host = api_host,
            version = api_version,
            uuid = uuid,
        ))
        .map_err(|why| format!("Unable to parse CloudLock API URL: {:?}", why))?;

        let cert = Self::get_cert_as_pem(&base_url, api_key)?;

        let cert = openssl::x509::X509::from_pem(cert.as_bytes())
            .map_err(|_| format!("Unable to load certificate from PEM"))?;

        Ok(Self {
            api_key: String::from(api_key),
            base_url,
            cert,
        })
    }

    fn get_cert_as_pem(base_url: &url::Url, api_key: &str) -> Result<String, String> {
        let url = base_url
            .join("config")
            .map_err(|why| format!("Unable to build URL: {:?}", why))?;

        let response = reqwest::Client::new()
            .get(&url.to_string())
            .bearer_auth(&api_key)
            .header("User-Agent", "CloudLock v1 HSM Client")
            .send()
            .map_err(|why| format!("Unable to do request for {}: {:?}", &url, why))?
            .json::<CloudLockConfig>()
            .map_err(|why| format!("Unable to deserialize response for {}: {:?}", &url, why))?;

        Ok(response.cert_pem)
    }

    fn do_request(&self, action: &str, payload: CloudLockPayload) -> Result<Blob, String> {
        let url = self
            .base_url
            .join(action)
            .map_err(|why| format!("Unable to build CloudLock URL: {:?}", why))?;

        let response = reqwest::Client::new()
            .post(&url.to_string())
            .bearer_auth(&self.api_key)
            .header("User-Agent", "CloudLock v1 HSM Client")
            .json(&payload)
            .send()
            .map_err(|why| format!("Unable to do request for {}: {:?}", &url, why))?
            .json::<CloudLockPayload>()
            .map_err(|why| format!("Unable to deserialize response for {}: {:?}", &url, why))?;

        base64::decode(&response.data)
            .map_err(|why| format!("Unable to decode response from Base64: {:?}", why))
    }
}

impl VirtualHSM for CloudLockHSM {
    fn encrypt(&self, blob: Blob) -> CryptoResult<Blob> {
        let data = base64::encode(&blob);
        let mut certs = openssl::stack::Stack::new().unwrap();
        certs
            .push(self.cert.to_owned())
            .map_err(|why| CryptoError::UnableToEncrypt(format!("{:?}", why)))?;
        let pkcs7 = openssl::pkcs7::Pkcs7::encrypt(
            &certs,
            &data.as_bytes(),
            openssl::symm::Cipher::aes_256_cbc(),
            openssl::pkcs7::Pkcs7Flags::empty(),
        )
        .map_err(|why| CryptoError::UnableToEncrypt(format!("{:?}", why)))?;

        let pem = pkcs7
            .to_pem()
            .map_err(|why| CryptoError::UnableToEncrypt(format!("{:?}", why)))?;
        Ok(pem)
    }

    fn decrypt(&self, blob: Blob) -> CryptoResult<Blob> {
        let _ = openssl::pkcs7::Pkcs7::from_pem(&blob)
            .map_err(|why| CryptoError::UnableToDecrypt(format!("{:?}", why)))?;

        self.do_request(
            "decrypt",
            CloudLockPayload {
                data: String::from_utf8(blob.to_owned())
                    .expect("Unable to cast payload for sending"),
            },
        )
        .map_err(|why| CryptoError::UnableToDecrypt(format!("{:?}", why)))
    }

    fn random_bytes(&self) -> CryptoResult<Blob> {
        let mut buf = [0; 128];
        rand_bytes(&mut buf).unwrap();

        Ok(buf.to_vec())
    }
}

#[test]
fn test_cloudlock_can_encrypt() {
    let hsm = CloudLockHSM::new(
        "0f6ed91e2e234bac8283cc4be656c729",
        "juGU4ZxTIrVCuHGPhQWlCxGmDwgiv354",
        "api.balena-dev.com",
        "v1",
    )
    .expect("Unable to initialise the CloudLock HSM");

    let random_bytes = "hello world".to_string().as_bytes().to_vec();

    let encrypted = hsm
        .encrypt(random_bytes.to_owned())
        .expect("Unable to encrypt bytes");

    println!("{}", String::from_utf8(encrypted.to_owned()).unwrap());

    assert_ne!(&encrypted, &random_bytes);

    let decrypted = hsm
        .decrypt(encrypted.to_owned())
        .expect("Unable to decrypt");

    assert_eq!(&random_bytes, &decrypted);
}
