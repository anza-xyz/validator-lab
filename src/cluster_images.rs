use {
    crate::{validator::Validator, ValidatorType},
    std::{error::Error, result::Result},
};

// Holds all validators.
// 1) Bootstrap
// 2) Standard Validator -> One image for each validator (not implemented yet)
// 3) RPC Node -> One image for each RPC node (not implemented yet)
// 4) Clients -> Each client has its own image (not implemented yet)

#[derive(Default)]
pub struct ClusterImages {
    bootstrap: Option<Validator>,
    validator: Option<Validator>,
    rpc: Option<Validator>,
    clients: Vec<Validator>,
}

impl ClusterImages {
    pub fn set_item(&mut self, item: Validator, validator_type: ValidatorType) {
        match validator_type {
            ValidatorType::Bootstrap => self.bootstrap = Some(item),
            ValidatorType::Standard => self.validator = Some(item),
            ValidatorType::RPC => self.rpc = Some(item),
            ValidatorType::Client(_) => self.clients.push(item),
        }
    }

    pub fn bootstrap(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.bootstrap
            .as_mut()
            .ok_or_else(|| "Bootstrap validator is not available".into())
    }

    pub fn validator(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.validator
            .as_mut()
            .ok_or_else(|| "Validator is not available".into())
    }

    pub fn rpc(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.rpc
            .as_mut()
            .ok_or_else(|| "Validator is not available".into())
    }

    pub fn client(&mut self, client_index: usize) -> Result<&mut Validator, Box<dyn Error>> {
        if self.clients.is_empty() {
            return Err("No Clients available".to_string().into());
        }
        self.clients
            .get_mut(client_index)
            .ok_or_else(|| "Client index out of bounds".to_string().into())
    }

    pub fn get_validators(&self) -> impl Iterator<Item = &Validator> {
        self.bootstrap
            .iter()
            .chain(self.validator.iter())
            .chain(self.rpc.iter())
            .filter_map(Some)
    }

    pub fn get_clients(&self) -> impl Iterator<Item = &Validator> {
        self.clients.iter()
    }

    pub fn get_clients_mut(&mut self) -> impl Iterator<Item = &mut Validator> {
        self.clients.iter_mut()
    }

    pub fn get_all(&self) -> impl Iterator<Item = &Validator> {
        self.get_validators().chain(self.get_clients())
    }
}
