use std::fmt::Display;
use std::mem;
use std::sync::Arc;
use log::{error, trace};
use reqrio::{tokio, Buffer, Method, Response};
use reqrio::tokio::io::AsyncWriteExt;
use reqrio::tokio::net::TcpStream;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::{DnsName, ServerName};
use tokio::io::{AsyncReadExt, ReadHalf, WriteHalf};
use tokio::sync::mpsc::Sender;
use tokio::task::{JoinError, JoinHandle};
use tokio_rustls::TlsConnector;
use uuid::Uuid;
use crate::error::{ProxyError, ProxyResult};
use crate::{gen_acceptor_for_sni, regex_find};
use crate::data::http::Request;

#[derive(Clone)]
pub enum Direction {
    ClientToServer,
    ServerToClient,
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::ClientToServer => f.write_str("ClientToServer"),
            Direction::ServerToClient => f.write_str("ServerToClient"),
        }
    }
}

pub struct ProxyParam {
    sid: String,
    sender: Sender<(Direction, Response)>,
    buffer: Buffer,
    data: Request,
    direction: Direction,
}

impl Clone for ProxyParam {
    fn clone(&self) -> Self {
        ProxyParam {
            sid: self.sid.clone(),
            sender: self.sender.clone(),
            buffer: Buffer::new(),
            data: Request::new(),
            direction: self.direction.clone(),
        }
    }
}

//
pub struct ProxyStream {
    //生成一个id以便区分流
    inbound: TcpStream,
    param: ProxyParam,
}

impl ProxyStream {
    pub fn new(inbound: TcpStream, sender: Sender<(Direction, Response)>) -> ProxyStream {
        ProxyStream {
            inbound,
            param: ProxyParam {
                sid: Uuid::new_v4().to_string(),
                sender,
                //初始化一个缓冲区
                buffer: Buffer::new(),
                data: Request::new(),
                direction: Direction::ClientToServer,
            },
        }
    }

    async fn copy<'a, I, O>(mut reader: ReadHalf<I>, mut writer: WriteHalf<O>, mut param: ProxyParam) -> JoinHandle<ProxyResult<()>>
    where
        I: AsyncReadExt + Send + Unpin + 'static,
        O: AsyncWriteExt + Send + Unpin + 'static,
    {
        tokio::spawn(async move {
            loop {
                param.buffer.read(&mut reader).await?;
                //及时把数据发送出去，减少延时
                writer.write_all(param.buffer.as_ref()).await?;
                if param.buffer.len() == 0 { break; } //读取长度为0时，此tcp连接已断开
                match param.direction {
                    Direction::ClientToServer => {
                        let method = param.buffer.as_ref().split(|&c| c == b' ' as u8).next();
                        if let Some(method) = method && Method::try_from(method).is_ok() {
                            //新的请求，把旧的的请求发到ui
                            let request = mem::replace(&mut param.data, Request::new());
                            param.sender.send((param.direction.clone(), request)).await?;
                        }
                        param.data.extend(&param.buffer)?;
                    }
                    Direction::ServerToClient => {
                        if param.data.extend(&param.buffer)? {
                            let response = mem::replace(&mut param.data, Response::new());
                            param.sender.send((param.direction.clone(), response)).await?;
                        }
                    }
                }
            }
            Ok::<(), ProxyError>(())
        })
    }

    async fn copy_io<I, O>(inbound: I, outbound: O, mut param: ProxyParam) -> ProxyResult<()>
    where
        I: AsyncReadExt + AsyncWriteExt + Send + Unpin + 'static,
        O: AsyncReadExt + AsyncWriteExt + Send + Unpin + 'static,
    {
        let res_func = |res: Result<ProxyResult<()>, JoinError>, direction: Direction| {
            match res {
                Ok(r) => match r {
                    Ok(()) => {}
                    Err(e) => error!("{}{}",direction,e.to_string())
                }
                Err(e) => error!("{}{}",direction,e.to_string())
            }
        };
        let (inbound_reader, inbound_writer) = tokio::io::split(inbound);
        let (outbound_reader, outbound_writer) = tokio::io::split(outbound);
        let rt1 = ProxyStream::copy(inbound_reader, outbound_writer, param.clone()).await;
        param.direction = Direction::ServerToClient;
        let rt2 = ProxyStream::copy(outbound_reader, inbound_writer, param).await;
        let (r1, r2) = tokio::join!(rt1,rt2);
        res_func(r1, Direction::ClientToServer);
        res_func(r2, Direction::ServerToClient);
        Ok(())
    }

    async fn handle_http(self) -> ProxyResult<()> {
        let http_prefix = b"http://";
        let start_pos = self.param.buffer.as_ref().windows(http_prefix.len()).position(|b| b == http_prefix).ok_or("获取HTTP地址失败")?;
        let end_pos = self.param.buffer.as_ref()[start_pos + http_prefix.len()..].iter().position(|b| *b == b'/').ok_or("获取HTTP地址失败")? + start_pos + http_prefix.len();
        println!("{} {}", start_pos, end_pos);
        //获取真实服务器地址，端口为80的会自动省略
        let addr = String::from_utf8(self.param.buffer.as_ref()[start_pos + http_prefix.len()..end_pos].to_vec())?;
        println!("{}", addr);
        let host = addr.split(":").next().unwrap();
        let port = match addr.contains(":") {
            true => addr.split(":").last().ok_or("获取端口失败")?.parse::<u16>()?,
            false => 80
        };
        // 这里我们就拿到了真实的服务器地址
        println!("{}:{}", host, port);
        //与真实服务器建立连接，并把两个stream相互复制
        let mut outbound = TcpStream::connect(format!("{}:{}", host, port)).await?;
        // outbound.write(&buffer[..len]).await?;
        if start_pos > 0 {
            outbound.write_all(&self.param.buffer[..start_pos]).await?;
        }
        outbound.write(&self.param.buffer.as_ref()[end_pos..]).await?;
        ProxyStream::copy_io(self.inbound, outbound, self.param).await
    }

    async fn handle_https(mut self) -> ProxyResult<()> {
        let info = String::from_utf8_lossy(self.param.buffer.as_ref()).to_string();
        let addr = regex_find("CONNECT (.*?) ", info.as_str())?;
        if addr.len() == 0 { return Err("获取HTTPS真实地址失败".into()); }
        self.inbound.write(b"HTTP/1.1 200 OK\r\n\r\n").await?;
        self.inbound.flush().await?;
        //从这里开始，两个stream之间交互的就是真实的https数据了
        let sni = addr[0].split(":").next().unwrap();
        trace!("已解析到https地址：{}；SNI：{}",addr[0],sni);
        let acceptor = gen_acceptor_for_sni(sni)?;
        let inbound = acceptor.accept(self.inbound).await?;
        let mut root_ca = RootCertStore::empty();
        root_ca.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let client_config = ClientConfig::builder().with_root_certificates(root_ca).with_no_client_auth();
        let outbound = TcpStream::connect(&addr[0]).await?;
        let connector = TlsConnector::from(Arc::new(client_config));
        let server_name = ServerName::DnsName(DnsName::try_from(sni.to_string())?);
        let outbound = connector.connect(server_name, outbound).await?;
        // //这里我们就实现了HTTPS解密，但是我们的根证书还没安装
        // //sudo cp sca.pem /etc/pki/ca-trust/source/anchors/
        // //sudo update-ca-trust
        ProxyStream::copy_io(inbound, outbound, self.param).await
    }

    pub async fn start(mut self) -> ProxyResult<()> {
        self.param.buffer.read(&mut self.inbound).await?;

        if self.param.buffer.as_ref().starts_with(b"CONNECT") {
            self.handle_https().await?;
        } else {
            self.handle_http().await?;
        }
        Ok(())
    }
}