pub mod proc_proto {
    include!(concat!(env!("OUT_DIR"), "/process_rpc_proto.rs"));
}

use self::proc_proto::{FuncCallReq, FuncCallResp};
use super::SharedInstance;
use crate::general::app;
use crate::general::app::app_shared::process_rpc::proc_proto::AppStarted;
use crate::general::app::app_shared::process_rpc_proto_ext::{ProcRpcExtKvReq, ProcRpcReqExt};
use crate::general::network::rpc_model::ProcRpcTaskId;
use crate::{
    general::network::rpc_model::{self, HashValue, MsgIdBind, ReqMsg, RpcCustom},
    modules_global_bridge::process_func::ModulesGlobalBrigeInstanceManager,
    result::WSResult,
    sys::LogicalModulesRef,
};
use async_trait::async_trait;
use parking_lot::Mutex;
use prost::Message;
use std::sync::Arc;
use std::{collections::HashMap, path::Path, time::Duration};
use tokio::sync::oneshot;

// const AGENT_SOCK_PATH: &str = "agent.sock";

fn clean_sock_file(path: impl AsRef<Path>) {
    let _ = std::fs::remove_file(path);
}

// pub struct ProcessRpcInner();

#[derive(Clone)]
pub struct ProcessRpc(Arc<app::View>);

impl ProcessRpc {
    pub fn new(app: app::View) -> Self {
        ProcessRpc(Arc::new(app))
    }
}

lazy_static::lazy_static! {
    static ref WATING_VERIFY: Mutex<HashMap<String, Vec<oneshot::Sender<AppStarted>>>>=Mutex::new(HashMap::new());
    static ref MODULES: Option<LogicalModulesRef>=None;
}

#[async_trait]
impl RpcCustom for ProcessRpc {
    type SpawnArgs = String;
    fn bind(a: String) -> tokio::net::UnixListener {
        clean_sock_file(&a);
        tokio::net::UnixListener::bind(&a).unwrap()
    }
    // fn deserialize(id: u16, buf: &[u8]) {
    //     let res = match id {
    //         1 => {
    //             let pack = proto::FuncCallResp::decode(buf);
    //             let _ = tokio::spawn(async move {
    //                 // return the result
    //             });
    //         }
    //         _ => unimplemented!(),
    //     };
    // }

    async fn verify(&self, buf: &[u8]) -> Option<HashValue> {
        let res = proc_proto::AppStarted::decode(buf);
        let res: proc_proto::AppStarted = match res {
            Ok(res) => res,
            Err(_) => {
                return None;
            }
        };

        unsafe {
            tracing::debug!("verify begin");
            // let appman = ProcessRpc::global_m_app_meta_manager();
            struct Defer;
            impl Drop for Defer {
                fn drop(&mut self) {
                    tracing::debug!("verify end");
                }
            }
            let _d = Defer;

            // TODO: add http available check
            // let ishttp = {
            //     let appmanmetas = appman.meta.read().await;
            //     let Some(app) = appmanmetas.get_app_meta(&res.appid).await else {
            //         tracing::warn!("app {} not found, invalid verify !", res.appid);
            //         return None;
            //     };
            //     app.contains_http_fn()
            // };
            // let with_http_port = res.http_port.is_some();
            // if ishttp && !with_http_port
            // // || (!ishttp && with_http_port) <<< seems ok
            // {
            //     return None;
            // }

            // update to the instance
            // let insman = ProcessRpc::global_m_instance_manager();
            let instance = self
                .0
                .instance_manager()
                .app_instances
                .get(&res.appid)
                .expect(&format!(
                    "instance should be inited before get the verify {}",
                    res.appid
                ));
            let Some(s): Option<&SharedInstance> = instance.value().as_shared() else {
                tracing::warn!("only receive the verify from the instance that is shared");
                return None;
            };
            if !s.0.set_verifyed(res.clone()) {
                return None;
            }
        }

        Some(HashValue::Str(res.appid))
    }

    fn handle_remote_call(
        &self,
        conn: &HashValue,
        msgid: u8,
        taskid: ProcRpcTaskId,
        buf: &[u8],
    ) -> bool {
        tracing::debug!("handle_remote_call: id: {}", msgid);
        // let _ = match id {
        //     4 => (),

        // };
        let err = match msgid {
            4 => match proc_proto::UpdateCheckpoint::decode(buf) {
                Ok(_req) => {
                    tracing::debug!("function requested for checkpoint, but we ignore it");
                    // let conn = conn.clone();
                    // let _ = tokio::spawn(async move {
                    //     unsafe {
                    //         let ins_man = ProcessRpc::global_m_instance_manager().unwrap();
                    //         ins_man.update_checkpoint(conn.as_str().unwrap()).await;
                    //     }
                    // });
                    return true;
                }
                Err(e) => e,
            },

            5 => match proc_proto::KvRequest::decode(buf) {
                Ok(req) => {
                    tracing::debug!("function requested for kv");
                    let proc_rpc = self.clone();
                    let srctaskid = req.fn_taskid();
                    let conn = conn.clone();
                    let _ = tokio::spawn(async move {
                        let proc_rpc_res = proc_rpc
                            .0
                            .kv_user_client()
                            .kv_requests(srctaskid, req.to_proto_kvrequests())
                            .await;
                        match proc_rpc_res {
                            Ok(mut res) => {
                                tracing::debug!("function kv request success, sending response");
                                match rpc_model::send_resp::<proc_proto::KvRequest>(
                                    conn,
                                    taskid,
                                    proc_proto::KvResponse::from(res.responses.pop().unwrap()),
                                )
                                .await
                                {
                                    Ok(_) => {
                                        tracing::debug!("send kv response success");
                                    }
                                    Err(e) => {
                                        tracing::warn!("send kv response failed: {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("function kv request failed, error: {:?}", e);
                            }
                        }
                    });
                    return true;
                }
                Err(e) => {
                    tracing::warn!(
                        "decode kv request failed with buf length: {}, parital content: {:?}",
                        buf.len(),
                        &buf[..20]
                    );
                    e
                }
            },
            id => {
                tracing::warn!("handle_remote_call: unsupported id: {}", id);
                return false;
            }
        };
        tracing::warn!("handle_remote_call error: {:?}", err);
        true
    }
}

impl MsgIdBind for proc_proto::AppStarted {
    fn id() -> u16 {
        1
    }
}

impl MsgIdBind for proc_proto::FuncCallReq {
    fn id() -> u16 {
        2
    }
}

impl MsgIdBind for proc_proto::FuncCallResp {
    fn id() -> u16 {
        3
    }
}

impl MsgIdBind for proc_proto::UpdateCheckpoint {
    fn id() -> u16 {
        4
    }
}

impl MsgIdBind for proc_proto::KvRequest {
    fn id() -> u16 {
        5
    }
}

impl MsgIdBind for proc_proto::KvResponse {
    fn id() -> u16 {
        6
    }
}

impl ReqMsg for FuncCallReq {
    type Resp = FuncCallResp;
}

impl ReqMsg for proc_proto::KvRequest {
    type Resp = proc_proto::KvResponse;
}

pub async fn call_func(
    srcfnid: proc_proto::FnTaskId,
    app: &str,
    func: &str,
    arg: String,
) -> WSResult<FuncCallResp> {
    rpc_model::call(
        FuncCallReq {
            src_task_id: Some(srcfnid),
            func: func.to_owned(),
            arg_str: arg,
        },
        HashValue::Str(app.into()),
        Duration::from_secs(120),
    )
    .await
}
