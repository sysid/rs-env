// #![allow(dead_code)]

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use anyhow::{anyhow, Result};
use camino::Utf8Path;

#[derive(Debug, Clone)]
pub struct NodeDataGraph {
    pub base_path: String,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct TreeNodeGraph {
    pub node_data: NodeDataGraph,
    pub parents: Vec<WeakTreeNodeRefGraph>,
    pub children: Vec<TreeNodeRefGraph>,
}

pub type WeakTreeNodeRefGraph = Weak<RefCell<TreeNodeGraph>>;
pub type TreeNodeRefGraph = Rc<RefCell<TreeNodeGraph>>;

#[allow(unused_variables)]
pub fn build_dag(directory_path: &Utf8Path) -> Result<Vec<Rc<RefCell<TreeNodeGraph>>>> {
    Err(anyhow!("Not implemented yet: build_dag()".to_string()))
}
