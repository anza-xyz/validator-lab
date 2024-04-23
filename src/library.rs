use {
    crate::{validator::Validator, ValidatorType},
    std::{error::Error, result::Result},
};

// Holds all validators.
// 1) Bootstrap
// 2) Standard Validator -> One image for each validator (not implemented yet)
// 3) RPC Node -> One image for each RPC node (not implemented yet)
// 4) Clients -> Each client has its own image (not implemented yet)

pub struct Library {
    validators: Vec<Option<Validator>>,
    _clients: Option<Vec<Validator>>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            validators: vec![None; 1],
            _clients: None,
        }
    }
}

impl Library {
    pub fn set_item(&mut self, item: Validator, validator_type: ValidatorType) {
        match validator_type {
            ValidatorType::Bootstrap => self.validators[0] = Some(item),
            _ => panic!("{validator_type} not implemented yet!"),
        }
    }

    pub fn bootstrap(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.validators
            .get_mut(0)
            .and_then(Option::as_mut)
            .ok_or_else(|| "No Bootstrap validator found.".to_string().into())
    }

    pub fn get_validators(&self) -> impl Iterator<Item = &Validator> {
        self.validators.iter().filter_map(Option::as_ref)
    }
}
