use super::StorageBackend;
use crate::config::S3Config;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{Client, Method};
use sha2::{Digest, Sha256};
use std::io;
use std::path::PathBuf;

type HmacSha256 = Hmac<Sha256>;

pub struct S3Storage {
    client: Client,
    cfg: S3Config,
}

impl S3Storage {
    pub fn new(cfg: &S3Config) -> Self {
        Self {
            client: Client::new(),
            cfg: cfg.clone(),
        }
    }

    fn object_url_and_host_path(&self, key: &str) -> (String, String, String) {
        let clean_key = key.trim_start_matches('/');
        let base = if self.cfg.endpoint.is_empty() {
            format!("https://s3.{}.amazonaws.com", self.cfg.region)
        } else {
            self.cfg.endpoint.trim_end_matches('/').to_string()
        };

        let host_part = base
            .strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))
            .unwrap_or(&base)
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();

        if self.cfg.path_style || self.cfg.bucket.is_empty() {
            let uri_path = if self.cfg.bucket.is_empty() {
                format!("/{}", clean_key)
            } else {
                format!("/{}/{}", self.cfg.bucket, clean_key)
            };
            (format!("{}{}", base, uri_path), host_part, uri_path)
        } else {
            let scheme = if base.starts_with("http://") {
                "http://"
            } else {
                "https://"
            };
            let vhost = format!("{}.{}", self.cfg.bucket, host_part);
            let uri_path = format!("/{}", clean_key);
            (format!("{}{}{}", scheme, vhost, uri_path), vhost, uri_path)
        }
    }

    fn sign_and_send(
        &self,
        method: Method,
        key: &str,
        payload: &[u8],
    ) -> Result<reqwest::RequestBuilder, io::Error> {
        let (url, host, uri_path) = self.object_url_and_host_path(key);
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let region = if self.cfg.region.is_empty() {
            "us-east-1"
        } else {
            self.cfg.region.as_str()
        };

        let payload_hash = hex::encode(Sha256::digest(payload));
        let canonical_headers = format!(
            "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            host, payload_hash, amz_date
        );
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";

        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method.as_str(),
            uri_path,
            canonical_headers,
            signed_headers,
            payload_hash
        );

        let scope = format!("{}/{}/s3/aws4_request", date_stamp, region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            scope,
            hex::encode(Sha256::digest(canonical_request.as_bytes()))
        );

        let k_secret = format!("AWS4{}", self.cfg.secret_key);
        let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes())?;
        let k_region = hmac_sha256(&k_date, region.as_bytes())?;
        let k_service = hmac_sha256(&k_region, b"s3")?;
        let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes())?);

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.cfg.access_key, scope, signed_headers, signature
        );

        Ok(self
            .client
            .request(method, url)
            .header("Host", host)
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header("Authorization", authorization))
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> io::Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn put(&self, key: &str, data: Bytes) -> io::Result<()> {
        let req = self
            .sign_and_send(Method::PUT, key, &data)?
            .body(data);
        let resp = req
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if !resp.status().is_success() {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("S3 PUT failed ({}): {}", st, txt),
            ));
        }
        Ok(())
    }

    fn local_path(&self, _key: &str) -> Option<PathBuf> {
        None
    }

    async fn get(&self, key: &str) -> io::Result<Bytes> {
        let req = self.sign_and_send(Method::GET, key, &[])?;
        let resp = req
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if !resp.status().is_success() {
            let st = resp.status();
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("S3 GET failed ({})", st),
            ));
        }
        resp.bytes()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    async fn delete(&self, key: &str) -> io::Result<()> {
        let req = self.sign_and_send(Method::DELETE, key, &[])?;
        let resp = req
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            let st = resp.status();
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("S3 DELETE failed ({})", st),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_s3_storage_put_get_delete_sigv4() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let store = std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::<String, Vec<u8>>::new()));
        let store_srv = store.clone();
        tokio::spawn(async move {
            for _ in 0..3 {
                if let Ok((mut sock, _)) = listener.accept().await {
                    let mut buf = vec![0u8; 8192];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                    assert!(raw.contains("AWS4-HMAC-SHA256 Credential=ak_test/"), "Missing SigV4 header: {}", raw);
                    let mut lines = raw.lines();
                    let req_line = lines.next().unwrap_or("");
                    let parts: Vec<&str> = req_line.split_whitespace().collect();
                    let method = parts.first().copied().unwrap_or("");
                    let path = parts.get(1).copied().unwrap_or("").to_string();
                    let header_end = raw.find("\r\n\r\n").unwrap_or(raw.len());
                    let body = if n > header_end + 4 { buf[header_end + 4..n].to_vec() } else { Vec::new() };

                    let mut map = store_srv.lock().await;
                    let resp_bytes = match method {
                        "PUT" => {
                            map.insert(path, body);
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()
                        }
                        "GET" => {
                            let val = map.get(&path).cloned().unwrap_or_default();
                            let hdr = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", val.len());
                            [hdr.into_bytes(), val].concat()
                        }
                        "DELETE" => {
                            map.remove(&path);
                            b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_vec()
                        }
                        _ => b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n".to_vec(),
                    };
                    let _ = sock.write_all(&resp_bytes).await;
                }
            }
        });

        let cfg = S3Config {
            endpoint: format!("http://{}", addr),
            region: "ap-northeast-2".into(),
            bucket: "test-bucket".into(),
            access_key: "ak_test".into(),
            secret_key: "sk_secret".into(),
            path_style: true,
        };
        let s3 = S3Storage::new(&cfg);
        assert!(s3.local_path("editor/test.webp").is_none());

        s3.put("editor/test.webp", Bytes::from_static(b"hello-webp")).await.unwrap();
        let got = s3.get("editor/test.webp").await.unwrap();
        assert_eq!(got.as_ref(), b"hello-webp");
        s3.delete("editor/test.webp").await.unwrap();
        assert!(store.lock().await.is_empty());
    }
}
