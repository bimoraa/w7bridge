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

    async fn on_progress( &self, params: ProgressNotificationParam, _context: NotificationContext<RoleClient>, ) {

        let _ = self.notifications.send(params);

    }

}
