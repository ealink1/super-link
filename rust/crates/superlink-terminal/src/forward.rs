//! Bounded loopback forwarding over the existing verified Rust SSH transport.
use crate::{
    SshClient,
    error::{Result, TerminalError},
    runtime::runtime,
};
use std::sync::Arc;
pub struct TcpForward {
    address: std::net::SocketAddr,
    task: tokio::task::JoinHandle<()>,
    client: SshClient,
}
impl SshClient {
    pub fn forward_tcp(self, host: String, port: u16) -> Result<TcpForward> {
        if host.is_empty() || port == 0 {
            return Err(TerminalError::Connect("转发目标无效".into()));
        }
        let rt = runtime()?;
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .map_err(|e| TerminalError::Connect(e.to_string()))?;
        let address = listener
            .local_addr()
            .map_err(|e| TerminalError::Connect(e.to_string()))?;
        let handle = Arc::clone(self.handle());
        let task=rt.spawn(async move {
            let mut streams=tokio::task::JoinSet::new();
            loop {
                tokio::select!{
                    accepted=listener.accept(),if streams.len()<16=>{
                        let Ok((mut socket,peer))=accepted else{break};
                        let handle=Arc::clone(&handle);let host=host.clone();
                        streams.spawn(async move {
                            if let Ok(Ok(channel))=tokio::time::timeout(std::time::Duration::from_secs(15),handle.channel_open_direct_tcpip(host,port.into(),peer.ip().to_string(),peer.port().into())).await {
                                let mut remote=channel.into_stream();
                                let _=tokio::io::copy_bidirectional(&mut socket,&mut remote).await;
                            }
                        });
                    },
                    _=streams.join_next(),if !streams.is_empty()=>{},
                }
            }
        });
        Ok(TcpForward {
            address,
            task,
            client: self,
        })
    }
}
impl TcpForward {
    pub fn address(&self) -> std::net::SocketAddr {
        self.address
    }
}
impl Drop for TcpForward {
    fn drop(&mut self) {
        self.task.abort();
        let _ = self.client.disconnect();
    }
}
