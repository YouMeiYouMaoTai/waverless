use crate::general::app::m_executor::Executor;
use crate::general::app::{AppMeta, AppMetaManager};
use crate::general::network::m_p2p::P2PModule;
use crate::logical_module_view_impl;
use crate::master::app::fddg::FDDGMgmt;
use crate::master::m_master::Master;
use crate::result::WSResult;
use crate::sys::{LogicalModule, LogicalModuleNewArgs, LogicalModulesRef};
use crate::util::JoinHandleWrapper;
use async_trait::async_trait;
use ws_derive::LogicalModule;

logical_module_view_impl!(MasterAppMgmtView);
// access general app
logical_module_view_impl!(MasterAppMgmtView, appmeta_manager, AppMetaManager);
logical_module_view_impl!(MasterAppMgmtView, p2p, P2PModule);
logical_module_view_impl!(MasterAppMgmtView, executor, Executor);
logical_module_view_impl!(MasterAppMgmtView, master, Option<Master>);

#[derive(LogicalModule)]
pub struct MasterAppMgmt {
    view: MasterAppMgmtView,
    pub fddg: FDDGMgmt,
}

#[async_trait]
impl LogicalModule for MasterAppMgmt {
    fn inner_new(args: LogicalModuleNewArgs) -> Self
    where
        Self: Sized,
    {
        Self {
            view: MasterAppMgmtView::new(args.logical_modules_ref.clone()),
            fddg: FDDGMgmt::new(),
        }
    }

    async fn init(&self) -> WSResult<()> {
        self.load_apps().await?;
        Ok(())
    }

    async fn start(&self) -> WSResult<Vec<JoinHandleWrapper>> {
        Ok(vec![])
    }
}

impl MasterAppMgmt {
    pub async fn update_app(&self, app_name: &str, app_meta: &AppMeta) -> WSResult<()> {
        for (fn_name, fn_meta) in app_meta.fns.iter() {
            self.fddg
                .add_fn_trigger((&app_name, app_meta.app_type), (&fn_name, &fn_meta))?;
        }
        Ok(())
    }

    async fn load_apps(&self) -> WSResult<()> {
        // load app triggers to fddg
        // - for each native apps
        for (app_name, app_meta) in &self.view.appmeta_manager().native_apps {
            self.update_app(app_name, app_meta).await?;
        }

        // - for each existing apps

        Ok(())
    }
}
