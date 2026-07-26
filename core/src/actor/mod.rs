pub mod handlers;
pub mod macros;
pub mod mapping;
pub mod search;
pub mod worker;

use matrix_sdk::ruma::api::client::uiaa::{AuthData, Password, UserIdentifier};
pub use worker::MatrixWorker;

use futures::channel::mpsc;
use matrix_sdk::Client;
use matrix_sdk::encryption::verification::{QrVerification, SasVerification};
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk_ui::timeline::Timeline;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use search::SearchIndex;
use selvedge_shared::event::ToShell;

pub struct MatrixActor {
    pub(crate) client: RefCell<Option<Client>>,
    pub(crate) event_sender: mpsc::UnboundedSender<ToShell>,
    pub(crate) active_timelines: RefCell<HashMap<OwnedRoomId, Rc<Timeline>>>,
    pub(crate) active_sas_verifications: RefCell<HashMap<String, SasVerification>>,
    pub(crate) active_qr_verifications: RefCell<HashMap<String, QrVerification>>,
    pub(crate) search_index: Rc<RefCell<SearchIndex>>,
}

impl MatrixActor {
    pub(crate) fn new(event_sender: mpsc::UnboundedSender<ToShell>) -> Self {
        Self {
            client: RefCell::new(None),
            event_sender,
            active_timelines: RefCell::new(HashMap::new()),
            active_sas_verifications: RefCell::new(HashMap::new()),
            active_qr_verifications: RefCell::new(HashMap::new()),
            search_index: Rc::new(RefCell::new(SearchIndex::default())),
        }
    }

    pub(crate) fn build_uiaa_auth_data(
        &self,
        session: String,
        password: String,
    ) -> Option<AuthData> {
        let client = self.client.borrow().clone()?;
        let identifier = UserIdentifier::UserIdOrLocalpart(
            client
                .user_id()
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );
        let mut uiaa_password = Password::new(identifier, password);
        uiaa_password.session = Some(session);

        Some(AuthData::Password(uiaa_password))
    }

    pub(crate) fn send_event(&self, event: ToShell) {
        let _ = self.event_sender.unbounded_send(event);
    }
}
