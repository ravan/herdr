use super::*;

impl HeadlessServer {
    /// Organization changes are independent of terminal/core projection changes.
    pub(super) fn send_organization_catalog(&mut self, client_id: u64) -> bool {
        let revision = self.app.state.organization.revision;
        if self
            .clients
            .get(&client_id)
            .is_some_and(|client| client.shell_organization_revision == Some(revision))
        {
            return true;
        }
        let message = match crate::protocol::endpoint::organization_message(
            &self.client_shell_boot_id,
            &self.app.state.organization,
        ) {
            Ok(message) => message,
            Err(error) => {
                warn!(client_id, %error, "failed to encode organization catalog");
                return false;
            }
        };
        if !self.send_to_client(client_id, message) {
            return false;
        }
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.shell_organization_revision = Some(revision);
        }
        true
    }
}
