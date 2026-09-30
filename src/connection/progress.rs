/*! peer progress를 bounded channel에 전달해. 요청별 token 필터링은 forward가 소유해. */

use rmcp::{ClientHandler, RoleClient, model::ProgressNotificationParam, service::NotificationContext};
use tokio::sync::broadcast;

#[derive(Clone)]
pub(crate) struct ProgressClient {

    pub notifications: broadcast::Sender<ProgressNotificationParam>,

}

impl Default for ProgressClient {

    fn default( ) -> Self {

        Self { notifications: broadcast::channel(32).0 }

    }

}

impl ClientHandler for ProgressClient {

    async fn on_progress( &self, mut params: ProgressNotificationParam, context: NotificationContext<RoleClient>, ) {

        // SDK 3.5는 notification metadata를 extensions로 옮겨. context.meta만 읽으면 payload를 잃어.
        let meta = params.meta.get_or_insert_with(Default::default);
        if let Some(extensions) = context.extensions.get::<rmcp::model::NotificationMetaObject>() {

            meta.extend(extensions.clone());

        }
        meta.extend(context.meta);
        let _ = self.notifications.send(params);

    }

}
