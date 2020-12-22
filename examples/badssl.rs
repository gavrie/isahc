//! This example contains a number of manual tests against badssl.com
//! demonstrating several dangerous SSL/TLS options.

use isahc::{config::SslOption, config::CustomTlsConfigurer, prelude::*};
use isahc::HttpClientBuilder;
use curl::Error;
use libc::c_void;
use openssl_sys::SSL_CTX;

#[derive(Clone)]
struct Bla;
impl CustomTlsConfigurer for Bla {
    unsafe fn configure(&self, ctx: *mut c_void) -> Result<(), Error> {
        if let Some(ssl_ver) = curl::Version::get().ssl_version() {
            if ssl_ver.starts_with("OpenSSL/") {
                let ctx = openssl::ssl::SslContextBuilder::from_ptr(ctx as *mut SSL_CTX).build();


                //let store = ctx.cert_store();
                println!("Open ssl :'{}', ctx: {:?}", ssl_ver, ctx.session_cache_size());
            }
        }
        Ok(())
    }
}

fn main() {
    let b = Bla;
    let x = HttpClientBuilder::new()
        .ssl_options(SslOption::DANGER_ACCEPT_INVALID_CERTS).danger_custom_tls_config(b);
    let c = x.build().unwrap();
    println!("{:?}",c.get("https://expired.badssl.com").unwrap().text());
    println!("{:?}",c.get("https://expired.badssl.com").unwrap().text());
/*
    // accept expired cert
    Request::get("https://expired.badssl.com")
        .ssl_options(SslOption::DANGER_ACCEPT_INVALID_CERTS)
        .body(())
        .unwrap()
        .send()
        .expect("cert should have been accepted");

    // accepting invalid certs alone does not allow invalid hosts
    Request::get("https://wrong.host.badssl.com")
        .ssl_options(SslOption::DANGER_ACCEPT_INVALID_CERTS)
        .body(())
        .unwrap()
        .send()
        .expect_err("cert should have been rejected");

    // accept cert with wrong host
    Request::get("https://wrong.host.badssl.com")
        .ssl_options(SslOption::DANGER_ACCEPT_INVALID_HOSTS)
        .body(())
        .unwrap()
        .send()
        .expect("cert should have been accepted");

    // accepting certs with wrong host alone does not allow invalid certs
    Request::get("https://expired.badssl.com")
        .ssl_options(SslOption::DANGER_ACCEPT_INVALID_HOSTS)
        .body(())
        .unwrap()
        .send()
        .expect_err("cert should have been rejected");
*/}
