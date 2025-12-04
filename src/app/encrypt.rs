use pasetors::{
    Local,
    claims::{Claims, ClaimsValidationRules},
    keys::SymmetricKey,
    local,
    token::{TrustedToken, UntrustedToken},
    version4::V4,
};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize)]
pub struct MessageClaim {
    msg: String,
}

#[derive(Clone, Debug)]
pub struct Secret(String);
impl Secret {
    pub fn get_key(&self) -> color_eyre::Result<SymmetricKey<V4>> {
        Ok(SymmetricKey::<V4>::from(self.0.as_bytes())?)
    }
}
impl FromStr for Secret {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Secret(s.to_string()))
    }
}

/// Encrypt if there's a key provided
pub fn try_encrypt_claims(text: String, secret: &Option<Secret>) -> color_eyre::Result<String> {
    let result = if let Some(secret) = secret {
        let mut claims = Claims::new()?;
        claims.add_additional("msg", text)?;
        local::encrypt(&secret.get_key()?, &claims, None, None)?
    } else {
        text
    };

    Ok(result)
}

pub fn try_decrypt_claims(text: &str, secret: &Option<Secret>) -> color_eyre::Result<String> {
    let result = if let Some(secret) = secret {
        let trusted = decrypt(&secret.get_key()?, text)?;
        let payload_json = trusted.payload();
        let parsed: MessageClaim = serde_json::from_str(payload_json)?;
        parsed.msg
    } else {
        text.to_owned()
    };

    Ok(result)
}

fn decrypt(key: &SymmetricKey<V4>, token: &str) -> color_eyre::Result<TrustedToken> {
    let untrusted = UntrustedToken::<Local, V4>::try_from(token)?;
    let rules = ClaimsValidationRules::default();
    let trusted = local::decrypt(key, &untrusted, &rules, None, None)?;
    Ok(trusted)
}
