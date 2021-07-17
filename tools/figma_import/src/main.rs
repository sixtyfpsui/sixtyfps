/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

use futures::prelude::*;
use std::collections::HashMap;
use std::fmt::Display;
use structopt::StructOpt;
use tokio::io::AsyncWriteExt;

#[derive(Debug, StructOpt)]
#[structopt(global_settings(&[structopt::clap::AppSettings::ColoredHelp]))]
struct Opt {
    /// Figma asscess token
    #[structopt(short = "t", long = "token")]
    token: String,
    /// If present, load the specific node id
    #[structopt(short = "n", long = "node")]
    node_id: Option<String>,
    /// If present, load the specific child node at the specified index
    #[structopt(long = "child")]
    child_index: Option<usize>,
    /// If set, don't connect to the network, but use the `figma_output/cache.json`
    #[structopt(long)]
    read_from_cache: bool,
    /// Figma file
    file: String,
}

mod figmatypes;
mod rendered;

fn fill_hash<'x>(hash: &mut HashMap<&'x str, &'x figmatypes::Node>, node: &'x figmatypes::Node) {
    let n = node.common();
    hash.insert(&n.id, node);
    for x in n.children.iter() {
        fill_hash(hash, x);
    }
}

#[derive(Debug)]
struct Error(String);
impl std::error::Error for Error {}
impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        self.0.fmt(f)
    }
}

async fn load_from_network(opt: &Opt) -> Result<figmatypes::File, Box<dyn std::error::Error>> {
    println!("Fetch document {}...", opt.file);
    let full_doc = reqwest::Client::new()
        .get(&format!("https://api.figma.com/v1/files/{}?geometry=paths", opt.file))
        .header("X-Figma-Token", opt.token.clone())
        .send()
        .await?
        .bytes()
        .await?;

    std::fs::create_dir_all("figma_output/images")?;
    std::fs::write("figma_output/cache.json", &full_doc)?;

    let r: figmatypes::File = serde_json::from_slice(&full_doc)?;

    use serde::Deserialize;
    #[derive(Deserialize)]
    struct ImageResult {
        meta: ImageResultMeta,
    };
    #[derive(Deserialize)]
    struct ImageResultMeta {
        images: HashMap<String, String>,
    };

    let i: ImageResult = reqwest::Client::new()
        .get(&format!("https://api.figma.com/v1/files/{}/images", opt.file))
        .header("X-Figma-Token", &opt.token)
        .send()
        .await?
        .json()
        .await?;

    println!("Fetch {} images ...", i.meta.images.len());
    let mut images = stream::iter(i.meta.images)
        .map(|(k, v)| async move {
            let mut resp = reqwest::Client::new().get(&v).send().await?.bytes_stream();
            let mut file = tokio::fs::File::create(format!("figma_output/images/{}", k)).await?;
            while let Some(bytes) = resp.next().await {
                file.write_all(&(bytes?)).await?;
            }
            Result::<(), Box<dyn std::error::Error>>::Ok(())
        })
        .buffer_unordered(8);
    while let Some(x) = images.next().await {
        x?
    }

    Ok(r)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let r = if !opt.read_from_cache {
        load_from_network(&opt).await?
    } else {
        let full_doc = std::fs::read("figma_output/cache.json")?;
        serde_json::from_slice(&full_doc)?
    };

    let mut nodeHash = HashMap::new();
    fill_hash(&mut nodeHash, &r.document);
    let doc = rendered::Document { nodeHash };

    if let figmatypes::Node::DOCUMENT(document) = &r.document {
        if let figmatypes::Node::CANVAS { node, prototypeStartNodeID, backgroundColor, .. } =
            &document.children[0]
        {
            let render_node = if let Some(node_id) = &opt.node_id {
                doc.nodeHash
                    .get(node_id.as_str())
                    .ok_or_else(|| Error(format!("Could not find node id {}", node_id)))?
            } else if let Some(child_index) = opt.child_index {
                node.children
                    .get(child_index)
                    .ok_or_else(|| Error(format!("The index {} does not exist", child_index)))?
            } else {
                doc.nodeHash
                    .get(
                        prototypeStartNodeID
                            .as_ref()
                            .ok_or_else(|| Error("No start node".into()))?
                            .as_str(),
                    )
                    .ok_or_else(|| Error("Start node not found".into()))?
            };
            let result = rendered::render(node.name.as_str(), render_node, *backgroundColor, &doc)?;

            std::fs::write("figma_output/main.60", &result)?;
        }
    }

    Ok(())
}
