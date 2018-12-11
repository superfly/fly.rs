use openssl::{ec, pkey::PKey, rsa, ssl, x509};
use std::collections::HashMap;
use std::sync::RwLock;

use super::REDIS_POOL;
use crate::kms::decrypt;

use r2d2_redis::redis;

lazy_static! {
    static ref CTX_STORE: RwLock<HashMap<String, ssl::SslContext>> = RwLock::new(HashMap::new());
}

pub fn get_cached_ctx(servername: &str) -> Option<ssl::SslContext> {
    debug!("trying to get cached ssl context for: {}", servername);
    if let Ok(store) = CTX_STORE.read() {
        if let Some(ctx) = store.get(servername) {
            return Some(ctx.clone());
        }
    }
    None
}

pub fn get_ctx(servername: &str) -> Result<Option<ssl::SslContext>, String> {
    debug!("getting ssl context for: {}", servername);
    if let Some(ctx) = get_cached_ctx(servername) {
        return Ok(Some(ctx));
    }

    let mut wildcard: Option<String> = None;
    let mut splitted = servername.split(".");
    let parts = splitted.clone().count();
    if parts > 2 {
        let first = splitted.next().unwrap();
        if first != "*" {
            let mut skipped = splitted.skip(parts - 3); // we've already read 1, so skip 1 less than 2 (-3)
            let wc = format!("*.{}.{}", skipped.next().unwrap(), skipped.next().unwrap());

            if let Some(ctx) = get_cached_ctx(wc.as_str()) {
                return Ok(Some(ctx));
            }
            wildcard = Some(wc);
        }
    }

    match REDIS_POOL.get() {
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
                            debug!("parsing fullchain...");
                            let pems =
                                x509::X509::stack_from_pem(c.get("fullchain").unwrap().as_slice())
                                    .unwrap();
                            debug!(
                                "setting certificate! {}",
                                String::from_utf8(pems[0].to_pem().unwrap()).unwrap()
                            );
                            builder.set_certificate(&pems[0]).unwrap();
                            debug!(
                                "adding extra chain cert: {}",
                                String::from_utf8(pems[1].to_pem().unwrap()).unwrap()
                            );
                            builder.add_extra_chain_cert(pems[1].clone()).unwrap();
                            debug!("decrypting private key!");
                            match decrypt(c.get("encrypted_private_key").unwrap().to_vec()) {
                                Err(e) => error!("could not decrypt private key: {}", e),
                                Ok(maybe_pem) => match maybe_pem {
                                    None => error!("no plaintext apparently... not sure why"),
                                    Some(pem) => {
                                        let typ =
                                            String::from_utf8(c.get("type").unwrap().to_vec())
                                                .unwrap();
                                        let pk = if typ == "rsa" {
                                            debug!("rsa, doing it.");
                                            PKey::from_rsa(
                                                rsa::Rsa::private_key_from_pem(pem.as_slice())
                                                    .unwrap(),
                                            )
                                            .unwrap()
                                        } else if typ == "ecdsa" {
                                            debug!("ecdsa, doing it.");
                                            PKey::from_ec_key(
                                                ec::EcKey::private_key_from_pem(pem.as_slice())
                                                    .unwrap(),
                                            )
                                            .unwrap()
                                        } else {
                                            warn!("unimplemented cert type!");
                                            unimplemented!();
                                        };
                                        debug!("setting private key");
                                        builder.set_private_key(&pk).unwrap();
                                        added = added + 1;
                                    }
                                },
                            }
                        }
                    }
                    if added == 0 {
                        if let Some(ref wc) = wildcard {
                            return get_ctx(wc.as_str());
                        } else {
                            return Ok(None);
                        }
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
    }
}
