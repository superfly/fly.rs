use openssl::{ec, pkey::PKey, rsa, ssl, x509};
use std::collections::HashMap;
use std::sync::RwLock;

use super::REDIS_POOL;
use crate::kms::decrypt;

use r2d2_redis::redis;

lazy_static! {
    static ref CTX_STORE: RwLock<HashMap<String, ssl::SslContext>> = RwLock::new(HashMap::new());
}

pub fn get_ctx(servername: &str) -> Result<Option<ssl::SslContext>, String> {
    match CTX_STORE.read() {
        Err(e) => Err(format!("{}", e)),
        Ok(store) => match store.get(servername) {
            Some(ctx) => Ok(Some(ctx.clone())),
            None => match REDIS_POOL.get() {
                Err(e) => Err(format!("{}", e)),
                Ok(conn) => match redis::pipe()
                    .cmd("HGETALL")
                    .arg(format!("certificate:{}:rsa", servername))
                    .cmd("HGETALL")
                    .arg(format!("certificate:{}:ecdsa", servername))
                    .query::<Vec<HashMap<String, Vec<u8>>>>(&*conn)
                {
                    Err(e) => Err(format!("{}", e)),
                    Ok(res) => match ssl::SslContextBuilder::new(ssl::SslMethod::tls()) {
                        Err(e) => Err(format!("{}", e)),
                        Ok(mut builder) => {
                            debug!("building ssl ctx");
                            let mut added = 0;
                            for c in res.iter() {
                                if !c.is_empty() {
                                    let pems = x509::X509::stack_from_pem(
                                        c.get("fullchain").unwrap().as_slice(),
                                    )
                                    .unwrap();
                                    builder.set_certificate(&pems[1]).unwrap();
                                    builder.add_extra_chain_cert(pems[0].clone()).unwrap();
                                    let pk_pem =
                                        decrypt(c.get("encrypted_private_key").unwrap().to_vec());
                                    let typ =
                                        String::from_utf8(c.get("type").unwrap().to_vec()).unwrap();
                                    let pk = if typ == "rsa" {
                                        PKey::from_rsa(
                                            rsa::Rsa::private_key_from_pem(pk_pem.as_slice())
                                                .unwrap(),
                                        )
                                        .unwrap()
                                    } else if typ == "ecdsa" {
                                        PKey::from_ec_key(
                                            ec::EcKey::private_key_from_pem(pk_pem.as_slice())
                                                .unwrap(),
                                        )
                                        .unwrap()
                                    } else {
                                        unimplemented!();
                                    };
                                    builder.set_private_key(&pk).unwrap();
                                    added = added + 1;
                                }
                            }
                            if added == 0 {
                                return Ok(None);
                            }
                            let ctx = builder.build();
                            debug!("built ctx!");
                            match CTX_STORE.write() {
                                Err(e) => error!("error writing to the ctx store! {}", e),
                                Ok(mut writer) => {
                                    writer.insert(servername.to_string(), ctx.clone());
                                    info!("inserted new context for {}", servername);
                                }
                            };
                            Ok(Some(ctx))
                        }
                    },
                },
            },
        },
    }
}
