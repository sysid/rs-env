#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::rc::{Rc, Weak};
use anyhow::{anyhow, Context, Result};
use camino::Utf8Path;
use regex::Regex;
use walkdir::WalkDir;
use crate::dlog;
use log::{debug, info};
use stdext::function_name;
use crate::tree::{NodeData, WeakTreeNodeRef};

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

pub fn build_dag(directory_path: &Utf8Path) -> Result<Vec<Rc<RefCell<TreeNodeGraph>>>> {
    Err(anyhow!(format!("{}: Not implemented yet", function_name!())))
}
