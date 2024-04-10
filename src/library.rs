use {
    crate::{validator::Validator, ValidatorType},
    std::{error::Error, result::Result},
};

pub struct Library {
    validators: Vec<Option<Validator>>,
    clients: Option<Vec<Validator>>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            validators: vec![None; 3],
            clients: None,
        }
    }
}

impl Library {
    pub fn set_item(&mut self, item: Validator, validator_type: ValidatorType) {
        match validator_type {
            ValidatorType::Bootstrap => self.validators[0] = Some(item),
            ValidatorType::Standard => self.validators[1] = Some(item),
            ValidatorType::RPC => self.validators[2] = Some(item),
            ValidatorType::Client => {
                if let Some(ref mut client_vec) = self.clients {
                    client_vec.push(item)
                } else {
                    self.clients = Some(vec![item])
                }
            }
        }
    }

    pub fn get_validators(&self) -> impl Iterator<Item = &Validator> {
        self.validators.iter().filter_map(Option::as_ref)
    }

    pub fn get_clients(&self) -> impl Iterator<Item = &Validator> {
        self.clients
            .as_ref()
            .into_iter()
            .flat_map(|clients| clients.iter())
    }

    pub fn get_all(&self) -> impl Iterator<Item = &Validator> {
        let individual_validators = self.validators.iter().filter_map(|v| v.as_ref());
        let client_iterators = self.clients.as_ref().into_iter().flat_map(|c| c.iter());
        individual_validators.chain(client_iterators)
    }

    pub fn bootstrap(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.validators
            .get_mut(0)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("No Bootstrap validator found.").into())
    }

    pub fn validator(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.validators
            .get_mut(1)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("No Validator found.").into())
    }

    pub fn rpc_node(&mut self) -> Result<&mut Validator, Box<dyn Error>> {
        self.validators
            .get_mut(2)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("No RPC node found.").into())
    }

    pub fn client(&mut self, client_index: usize) -> Result<&mut Validator, Box<dyn Error>> {
        if let Some(ref mut clients) = self.clients {
            clients
                .get_mut(client_index)
                .ok_or_else(|| format!("Client index out of bounds").into())
        } else {
            Err(format!("No Clients available").into())
        }
    }
}
