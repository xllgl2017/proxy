mod cert;
mod error;
// mod socks5;
mod proxy;
mod data;
mod gui;

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;
use egui::ViewportBuilder;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::file::FileAppender;
use log4rs::Config;
use log4rs::config::{Appender, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;
use log::{debug, error, info, trace, LevelFilter};
use reqrio::Response;
use rustls::ServerConfig;
use rustls_pemfile::Item;
use rustls_pki_types::PrivateKeyDer;
use tokio::net::TcpListener;
use tokio::sync;
use tokio::sync::mpsc::Receiver;
use tokio::time::sleep;
use tokio_rustls::TlsAcceptor;
use crate::error::ProxyResult;
use crate::gui::ProxyView;
use crate::proxy::{Direction, ProxyStream};
// fn main() {
//     let viewport = ViewportBuilder::default()
//         .with_title("Proxy").with_inner_size((1200.0, 6000.0));
//     let mut native_options = eframe::NativeOptions::default();
//     native_options.viewport=viewport;
//     eframe::run_native("Proxy", native_options, Box::new(|cc| ProxyView::new(cc))).unwrap();
// }

#[tokio::main]
async fn main() {
    tokio::spawn(async {
        init_log4rs().unwrap();
        start_server().await.unwrap();
    });
    sleep(Duration::from_secs(100000)).await;
    // start_socks5_server().await.unwrap()
}

fn init_log4rs() -> ProxyResult<()> {
    let coder = PatternEncoder::new("{h({d(%Y-%m-%d %H:%M:%S)} [{f}:{L}] {l:<6})} {M}:{m}{n}");
    let stdout = ConsoleAppender::builder().encoder(Box::new(coder.clone())).build();
    let requests = FileAppender::builder().encoder(Box::new(coder)).build("target/log/proxy.log")?;
    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("requests", Box::new(requests)))
        .logger(Logger::builder().build("rustls", LevelFilter::Error))
        .build(Root::builder().appender("stdout").appender("requests").build(LevelFilter::Trace))?;

    log4rs::init_config(config)?;
    Ok(())
}


//我们先在本地启动一个服务，监听7090端口
async fn start_server() -> ProxyResult<()> {
    let (sx, rx) = sync::mpsc::channel(1024);
    tokio::spawn(async move {
        receive_data(rx).await;
    });
    info!("在本地0.0.0.0:7090建立一个Tcp端口监听服务");
    let listen = TcpListener::bind("0.0.0.0:7090").await?;
    loop {
        //接受一个新连接
        let (stream, addr) = listen.accept().await?;
        debug!("来自{}的新连接",addr);
        //启动一个线程，避免造成其他连接阻塞，影响网络体验
        let sender = sx.clone();
        tokio::spawn(async move {
            ProxyStream::new(stream, sender).start().await.unwrap_or_else(|e| error!("{}",e.to_string()));
        });
    }
}
//到目前为止，我们没有做区分stream
async fn receive_once(rx: &mut Receiver<(Direction, Response)>) -> Option<()> {
    let (direction, data) = rx.recv().await?;
    match direction {
        Direction::ClientToServer => println!("{}", data.header()),
        Direction::ServerToClient => println!("{}", data.header()),
    }
    None
}

async fn receive_data(mut rx: Receiver<(Direction, Response)>) {
    loop {
        let _ = receive_once(&mut rx).await;
    }
}


#[allow(dead_code)]
fn regex_find(rex: &str, context: &str) -> ProxyResult<Vec<String>> {
    let regx = regex::RegexBuilder::new(rex).build()?;
    let mut res = vec![];
    for re in regx.captures_iter(context) {
        let mut r = vec![];
        for index in 0..re.len() {
            r.push(re[index].to_string());
        }
        if r.len() > 1 { r.remove(0); }
        res.extend(r);
    };
    Ok(res)
}


//这里需要实现一个TlsAcceptor才能解密
fn gen_acceptor_for_sni(sni: impl AsRef<str>) -> ProxyResult<TlsAcceptor> {
    //这里先要生成证书
    //在top命令中我们看到我们的程序在建立连接的时候cpu占用很高。这个是证书生成时占用的，这里我们做一个证书缓存
    let crt_path = format!("target/tmp/certs/{}.pem", sni.as_ref());
    let key_path = format!("target/tmp/certs/{}.key", sni.as_ref());
    let (sni_bs, key_bs) = if std::fs::exists(crt_path.as_str())? {
        let sni_bs = std::fs::read(crt_path.as_str())?;
        let key_bs = std::fs::read(key_path.as_str())?;
        (sni_bs, key_bs)
    } else {
        trace!("正在为{}生成证书",sni.as_ref());
        let (pem, key) = cert::gen_cert_for_sni(sni.as_ref(), "sca.pem", "sca.key")?;
        let sni_bs = pem.into_bytes();
        let key_bs = key.into_bytes();
        std::fs::write(crt_path.as_str(), sni_bs.as_slice())?;
        std::fs::write(key_path.as_str(), key_bs.as_slice())?;
        (sni_bs, key_bs)
    };


    let mut reader = BufReader::new(sni_bs.as_slice());
    let item = rustls_pemfile::read_one(&mut reader).transpose().ok_or("读取证书失败")??;
    let sni_cert = match item {
        Item::X509Certificate(cert) => cert,
        _ => return Err("不支持的证书".into()),
    };
    let mut reader = BufReader::new(key_bs.as_slice());
    let item = rustls_pemfile::read_one(&mut reader).transpose().ok_or("读取证书密钥失败")??;
    let sni_key = match item {
        Item::Pkcs1Key(key) => PrivateKeyDer::Pkcs1(key),
        Item::Pkcs8Key(key) => PrivateKeyDer::Pkcs8(key),
        Item::Sec1Key(key) => PrivateKeyDer::Sec1(key),
        _ => return Err("不支持的证书密钥类型".into()),
    };
    let config = ServerConfig::builder_with_protocol_versions(&rustls::ALL_VERSIONS)
        .with_no_client_auth().with_single_cert(vec![sni_cert], sni_key)?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    Ok(acceptor)
}