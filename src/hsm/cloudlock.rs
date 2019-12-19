use crate::config_json::ConfigJson;
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
    api_root_cert: Option<String>,
    base_url: url::Url,
    cert: openssl::x509::X509,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct CloudLockConfig {
    #[serde(rename = "cert")]
    pub cert_pem: String,
}

impl CloudLockHSM {
    pub fn from_config(config: &ConfigJson, api_version: &str) -> Result<Self, String> {
        let uuid = &config.uuid;
        let api_endpoint = &config
            .get_api_endpoint()
            .expect("Unable to get the API endpoint from config.json");
        let api_key = &config
            .get_api_key_for_endpoint(&api_endpoint)
            .expect("Unable to get the API key from config.json");
        let api_root_pem = config.get_api_root_certificate();

        Self::new(&uuid, &api_key, &api_endpoint, api_version, api_root_pem)
    }

    pub fn new(
        uuid: &str,
        api_key: &str,
        api_endpoint: &str,
        api_version: &str,
        api_root_ca_pem: Option<String>,
    ) -> Result<Self, String> {
        let base_url = url::Url::parse(api_endpoint)
            .and_then(|url| {
                url.join(&format!(
                    "/cloudlock/{version}/{uuid}/",
                    version = api_version,
                    uuid = uuid,
                ))
            })
            .map_err(|_| "Unable to parse API endpoint".to_string())?;

        let cert = Self::get_cert_as_pem(&base_url, api_key, &api_root_ca_pem)?;

        let cert = openssl::x509::X509::from_pem(cert.as_bytes())
            .map_err(|_| format!("Unable to load certificate from PEM"))?;

        Ok(Self {
            api_key: String::from(api_key),
            api_root_cert: api_root_ca_pem,
            base_url,
            cert,
        })
    }

    fn build_reqwest_client(root_cert: &Option<String>) -> Result<reqwest::Client, String> {
        let mut builder = reqwest::ClientBuilder::new();

        if let Some(pem) = root_cert {
            builder = builder
                .add_root_certificate(reqwest::Certificate::from_pem(pem.as_bytes()).unwrap());
        };

        let client = builder
            .build()
            .map_err(|why| format!("Unable to build client for API: {:?}", why))?;
        Ok(client)
    }

    fn get_cert_as_pem(
        base_url: &url::Url,
        api_key: &str,
        root_cert: &Option<String>,
    ) -> Result<String, String> {
        let url = base_url
            .join("config")
            .map_err(|why| format!("Unable to build URL: {:?}", why))?;

        let client = Self::build_reqwest_client(root_cert)?;

        let response = client
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

        let client = Self::build_reqwest_client(&self.api_root_cert)?;
        let response = client
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
    let config_json = "{
        \"uuid\": \"0f6ed91e2e234bac8283cc4be656c729\",
        \"apiEndpoint\": \"https://api.balena-dev.com\",
        \"deviceApiKeys\": {
            \"api.balena-dev.com\": \"juGU4ZxTIrVCuHGPhQWlCxGmDwgiv354\"
        },
        \"balenaRootCA\": \"LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tDQpNSUlFMWpDQ0FyNmdBd0lCQWdJVUIrNDJRemF5N0M5aVl2dE9FdlRuRU1lOEIzZ3dEUVlKS29aSWh2Y05BUUVMDQpCUUF3SERFYU1CZ0dBMVVFQXhNUlltRnNaVzVoTFdSbGRpNWpiMjBnUTBFd0hoY05NVGt4TWpBMU1UUTFORE01DQpXaGNOTWpreE1qQTFNVFExTkRNNVdqQWNNUm93R0FZRFZRUURFeEZpWVd4bGJtRXRaR1YyTG1OdmJTQkRRVENDDQpBaUl3RFFZSktvWklodmNOQVFFQkJRQURnZ0lQQURDQ0Fnb0NnZ0lCQVBiejJsR1QxeDhhRGxMZ216cktrWDh3DQpaV2JKNEhnY3RlM3krYlM0RE1iVVhjanVDSEtRcjM0M1NidkJ1YUJVWEMxSzlEc1VUc0JTZUpUWXlZbmFXNmtYDQpxMFNpd0RTdkpxK0pmY09vRTYrZDgwZ25YVzVRUlhnNm5oZGF6TzNsOHBBazNWeGtYMC84TTdFME1XbkF2eGRXDQp1cCtNMjI3QUUrbk5ub2tRekZzYWZNWjUxYVZ2RktTRnBieEgrNVUvWWRTRmdvcXoyTGpyamZ6MVdFcE5BVVdnDQp6anN5eHpCV0x0M1Vaa0NhQkFuRFR1NG5uOHEwRjFzT3hYenR4cXFJMHpyNVpTaURESnRWcVlFZ3F5YUgySVRFDQpVM0d1UGt6dG5scml4ZEVTZlNYUG1BS2xGYzd1emRMSis2TjZockFFR2plZStkYUtjTHdOdVZ0Y1hvdkN0OXFsDQpNZm43RVZYU2RteHBobTBGMEJkM3RpZ2lvT1hFMkdTbDVlUlBHbVBTbEJnc3pMVUJQWlRlUk9RdjBETXNoVmdJDQpBdlczTzB4VGJveTZPYytSZ0VoSVp1eldkMjV4S3JtRE1hVG1oOEM2bHVnUkZaTkhCb2kxeHU4dDFrakoraHQ3DQowK3NsdFRIekZ3ZXBETnVCVG0rTE5vck9QaFExdUNTSCtLa3owRkJ2Vm84c21HZWsrZVV4bklUWkt5R0xqZWVsDQoxTmE4OFduc21ZK1p1YURxZ1Jhdzl2SEh1dTNZaGc0WUZpUHI0OUs1UVpRRFhFZjY1dlYydS9HUnZzOEM3VWk2DQpEMGtET2ZNTDdWVXRsR1VQczllcDFPanpOK1FDcXRkcTdPZWdqKzY4MWcyU2VIbnYzbmFEaVVVRTFRbkliNTRmDQpkMHJoNHA5VmNlYlpjK2lsOXNtRkFnTUJBQUdqRURBT01Bd0dBMVVkRXdRRk1BTUJBZjh3RFFZSktvWklodmNODQpBUUVMQlFBRGdnSUJBQWVwZGpmSHVMUUlKZHA3ZVVCQzdERWZUUHlRNFBMNjltMzVvZzNhVVRjRy93T2tQZEVLDQpJek9EMGZCbVRxZVlxVGpTNGo4aFA1WmpmTGlPYmhLTWpMaERQU3htaEhGVFdFOEdZVjNjcTlXeU4xS2xObTV2DQpyRVUzQ0t1eUxlVk92ZXhOL3FMcFdwRXFmNVF1b2hkcU5vbDMzK1hNdkNrei9JK2tJVzBUTXJQaHdTZThOR3Q1DQp6TXlCM3IyOElhVEQ1c0sxeGMwUzVrRWMvVFQwT0d1Q0VpaUcvVmFBTWFjS05MS3RUWmF3Ukk0blV0UFc1THNTDQpieU41bTdWcTNGR25Qb1hscktTVmc5NEhYbWZuNCtzYlJmV3Zpb1l5bVdLblJiWmNFejNZU2w2MlNWY2pVUkpUDQovWTI4ZEg5QkFPVlFRd0dpbVNKY0F0NGE0R0d4VEtOQUwxVHkrelhpdXhqa2lWRDRJQko5RGRNTkdjcE5xNmVrDQpqelIzdDgxLys2WTNnMDBXUDBadUE4TG84OWRoN2ZMdWVwZ3NpU1NVTklwQUxJQTgwN1BnQVlINy9tWnFrUGtqDQpsWEhMUEhzRGhtMUhnWmkzQTk3VHlJL1Y5NHloZ0RUQ29mUnptMVZlR3RPYUpuZDQ1amRZaG5YcVIxSmFQVjQ5DQpaaXJvWGZyVkV2aTFVV2ZackJEc2VXODZRZDYzVmtIb2N0Q2sxdk5DRFdlMlRTSHhsaWFTRTl2ZWFXbUlNNUErDQpFWU9oQjMyRWE5VUVMZFdSOXN4TnZaUkJiM0JvbU0vY3BMSVVLb1JkYmxHTmdLTkN4RXh2TUNnQ3k5OUV3YklRDQpxOFRUR1RCWEFjQ014T2tTbWsxVUNTTTlmMXdHUzh2aXNVemNyb3M1VzdxS1dienhLck1jeWpaOA0KLS0tLS1FTkQgQ0VSVElGSUNBVEUtLS0tLQ0K\"
    }";

    let config = ConfigJson::from_json(config_json).expect("Unable to parse config JSON");

    let hsm =
        CloudLockHSM::from_config(&config, "v1").expect("Unable to initialise the CloudLock HSM");

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
