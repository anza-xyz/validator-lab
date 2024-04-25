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
    _validator: Option<Validator>,
    _rpc: Option<Validator>,
    _clients: Vec<Validator>,
}

impl ClusterImages {
    pub fn set_item(&mut self, item: Validator, validator_type: ValidatorType) {
        match validator_type {
            ValidatorType::Bootstrap => self.bootstrap = Some(item),
            _ => panic!("{validator_type} not implemented yet!"),
        }
    }

    pub fn bootstrap(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.bootstrap
            .as_mut()
            .ok_or_else(|| "Bootstrap validator is not available".into())
    }

    pub fn get_validators(&self) -> impl Iterator<Item = &Validator> {
        self.bootstrap
            .iter()
            .chain(self._validator.iter())
            .chain(self._rpc.iter())
            .filter_map(Some)
    }
}
