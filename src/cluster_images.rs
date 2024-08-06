use {
    crate::{node::Node, NodeType},
    std::{error::Error, result::Result},
};

// Holds all validators.
// 1) Bootstrap
// 2) Standard Validator -> One image for each validator (not implemented yet)
// 3) RPC Node -> One image for each RPC node (not implemented yet)
// 4) Clients -> Each client has its own image (not implemented yet)

#[derive(Default)]
pub struct ClusterImages {
    bootstrap: Option<Node>,
    validator: Option<Node>,
    rpc: Option<Node>,
    clients: Vec<Node>,
}

impl ClusterImages {
    pub fn set_item(&mut self, item: Node) {
        match item.node_type() {
            NodeType::Bootstrap => self.bootstrap = Some(item),
            NodeType::Standard => self.validator = Some(item),
            NodeType::RPC => self.rpc = Some(item),
            NodeType::Client(_, _) => self.clients.push(item),
        }
    }

    pub fn bootstrap(&mut self) -> Result<&mut Node, Box<dyn Error>> {
        self.bootstrap
            .as_mut()
            .ok_or_else(|| "Bootstrap validator is not available".into())
    }

    pub fn validator(&mut self) -> Result<&mut Node, Box<dyn Error>> {
        self.validator
            .as_mut()
            .ok_or_else(|| "Validator is not available".into())
    }

    pub fn rpc(&mut self) -> Result<&mut Node, Box<dyn Error>> {
        self.rpc
            .as_mut()
            .ok_or_else(|| "Validator is not available".into())
    }

    pub fn client(&mut self, client_index: usize) -> Result<&mut Node, Box<dyn Error>> {
        if self.clients.is_empty() {
            return Err("No Clients available".to_string().into());
        }
        self.clients
            .get_mut(client_index)
            .ok_or_else(|| "Client index out of bounds".to_string().into())
    }

    pub fn get_validators(&self) -> impl Iterator<Item = &Node> {
        self.bootstrap
            .iter()
            .chain(self.validator.iter())
            .chain(self.rpc.iter())
            .filter_map(Some)
    }

    pub fn get_clients(&self) -> impl Iterator<Item = &Node> {
        self.clients.iter()
    }

    pub fn get_clients_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.clients.iter_mut()
    }

    pub fn get_all(&self) -> impl Iterator<Item = &Node> {
        self.get_validators().chain(self.get_clients())
    }
}
