/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#![allow(unused)]

use std::collections::HashMap;
use std::f32::EPSILON;

use derive_more::*;
use serde::Deserialize;
use smart_default::SmartDefault;

#[derive(Debug, Deserialize)]
pub struct File {
    pub name: String,
    pub lastModified: Option<String>,
    pub thumbnailURL: Option<String>,
    pub version: String,
    pub document: Node,
    pub components: HashMap<String, Component>,
    //schemaVersion: 0,
    styles: HashMap<String, Style>,
}

#[derive(Debug, Deserialize)]
pub struct Component {
    pub key: String,
    pub file_key: Option<String>,
    pub node_id: Option<String>,
    pub thumbnail_url: Option<String>,
    pub name: String,
    pub description: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    //pub user : User,
    //pub containing_frame : Option<FrameInfo>,
    //pub containing_page Option<PageInfo>,
}

#[derive(Debug, Deserialize, Default)]
pub struct NodeCommon {
    pub id: String,
    pub name: String,
    #[serde(default = "return_true")]
    pub visible: bool,
    #[serde(default)]
    pub children: Vec<Node>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LayoutConstraint {
    pub vertical: String,
    pub horizontal: String,
}

fn return_true() -> bool {
    true
}
fn return_one() -> f32 {
    1.
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
pub struct Color {
    pub a: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub fn is_transparent(&self) -> bool {
        self.a <= f32::EPSILON
    }
}

// Sometimes figma is having null for coordinate for some reason, just ignore that and consider it is tempty
fn deserialize_or_default<'de, T: Default + Deserialize<'de>, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<T, D::Error> {
    Ok(T::deserialize(de).unwrap_or_default())
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rectangle {
    pub fn origin(self) -> Vector {
        Vector { x: self.x, y: self.y }
    }
    pub fn size(self) -> Vector {
        Vector { x: self.width, y: self.height }
    }
}

#[derive(Debug, Deserialize, Default, Clone, Copy, Add, AddAssign, Neg, Sub, SubAssign)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Deserialize)]
pub struct Paint {
    pub r#type: String,
    #[serde(default = "return_true")]
    pub visible: bool,
    #[serde(default = "return_one")]
    pub opacity: f32,
    pub color: Option<Color>,
    pub blendMode: BlendMode,
    #[serde(default)]
    pub gradientHandlePositions: Vec<Vector>,
    #[serde(default)]
    pub gradientStops: Vec<ColorStop>,
    pub scaleMode: Option<String>,
    pub imageTransform: Option<Transform>,
    pub imageRef: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ColorStop {
    pub position: f32,
    pub color: Color,
}

#[derive(Debug, Deserialize)]
pub struct LayoutGrid {
    pub pattern: String,
    pub sectionSize: f32,
    pub visible: bool,
    pub color: Color,
    pub alignment: String,
    pub gutterSize: f32,
    pub offset: f32,
    pub count: f32,
}

#[derive(Debug, Deserialize)]
pub struct Effect {
    pub r#type: String,
    pub visible: bool,
    pub radius: f32,
    pub color: Option<Color>,
    pub blendMode: Option<BlendMode>,
    pub offset: Option<Vector>,
}

#[derive(Debug, Deserialize, SmartDefault)]
#[serde(default)]
pub struct TypeStyle {
    pub fontFamily: String,
    pub fontPostScriptName: Option<String>,
    pub paragraphSpacing: f32,
    pub paragraphIndentNumber: f32,
    pub italic: bool,
    pub fontWeight: f32,
    pub fontSize: f32,
    #[default("ORIGINAL")]
    pub textCase: String,
    #[default("NONE")]
    pub textDecoration: String,
    pub textAlignHorizontal: String,
    pub textAlignVertical: String,
    pub letterSpacing: f32,
    pub fills: Vec<Paint>,
    pub lineHeightPx: f32,
    #[default(100.)]
    pub lineHeightPercent: f32,
    pub lineHeightPercentFontSize: f32,
    pub lineHeightUnit: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Path {
    pub path: String,
    pub windingRule: String,
}

type Transform = [[f32; 3]; 2];

type BlendMode = String;
type EasingType = String;

#[derive(Debug, Deserialize, Default)]
pub struct Frame {
    #[serde(flatten)]
    pub node: NodeCommon,
    #[serde(default)]
    pub locked: bool,
    pub background: Vec<Paint>,
    pub backgroundColor: Color,
    #[serde(default)]
    pub exportSettings: Vec<ExportSetting>,
    pub blendMode: BlendMode,
    #[serde(default)]
    pub preserveRatio: bool,
    pub constraints: LayoutConstraint,
    pub transitionNodeID: Option<String>,
    pub transitionDuration: Option<f32>,
    pub transitionEasing: Option<EasingType>,
    #[serde(default = "return_one")]
    pub opacity: f32,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub absoluteBoundingBox: Rectangle,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub size: Option<Vector>,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub relativeTransform: Option<Transform>,
    pub clipsContent: bool,
    #[serde(default)]
    pub layoutGrids: Vec<LayoutGrid>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub isMask: bool,
    #[serde(default)]
    pub isMaskOutline: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExportSetting {
    pub suffix: String,
    pub format: String,
    pub constraint: Constraint,
}

#[derive(Debug, Deserialize, Default)]
pub struct Constraint {
    pub r#type: String,
    pub value: f32,
}

#[derive(Debug, Deserialize, SmartDefault)]
#[serde(default)]
pub struct VectorNode {
    #[serde(flatten)]
    pub node: NodeCommon,
    pub locked: bool,
    pub exportSettings: Vec<ExportSetting>,
    pub blendMode: BlendMode,
    pub preserveRatio: bool,
    pub constraints: LayoutConstraint,
    pub transitionNodeID: Option<String>,
    pub transitionDuration: Option<f32>,
    pub transitionEasing: Option<EasingType>,
    #[default(1.)]
    pub opacity: f32,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub absoluteBoundingBox: Rectangle,
    pub effects: Vec<Effect>,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub size: Option<Vector>,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub relativeTransform: Option<Transform>,
    pub isMask: bool,
    pub fills: Vec<Paint>,
    pub fillGeometry: Vec<Path>,
    pub strokes: Vec<Paint>,
    pub strokeWeight: f32,
    #[default("NONE")]
    pub strokeCap: String,
    #[default("MITER")]
    pub strokeJoin: String,
    pub strokeDashes: Vec<f32>,
    #[default(28.96)]
    pub strokeMiterAngle: f32,
    pub strokeGeometry: Vec<Path>,
    pub strokeAlign: String,
    pub styles: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Node {
    DOCUMENT(NodeCommon),
    CANVAS {
        #[serde(flatten)]
        node: NodeCommon,
        backgroundColor: Color,
        prototypeStartNodeID: Option<String>,
        #[serde(default)]
        exportSettings: Vec<ExportSetting>,
    },
    FRAME(Frame),
    GROUP(Frame),
    VECTOR(VectorNode),
    BOOLEAN_OPERATION {
        #[serde(flatten)]
        vector: VectorNode,
        booleanOperation: String,
    },
    STAR(VectorNode),
    LINE(VectorNode),
    ELLIPSE(VectorNode),
    REGULAR_POLYGON(VectorNode),
    RECTANGLE {
        #[serde(flatten)]
        vector: VectorNode,
        cornerRadius: Option<f32>,
        #[serde(default)]
        rectangleCornerRadii: Vec<f32>,
    },
    TEXT {
        #[serde(flatten)]
        vector: VectorNode,
        characters: String,
        style: TypeStyle,
        characterStyleOverrides: Vec<f32>,
    },
    SLICE {
        #[serde(flatten)]
        node: NodeCommon,
        #[serde(default)]
        exportSettings: Vec<ExportSetting>,
        #[serde(deserialize_with = "deserialize_or_default")]
        absoluteBoundingBox: Rectangle,
        #[serde(deserialize_with = "deserialize_or_default")]
        size: Option<Vector>,
        #[serde(deserialize_with = "deserialize_or_default")]
        relativeTransform: Option<Transform>,
    },
    COMPONENT(Frame),
    INSTANCE {
        #[serde(flatten)]
        frame: Frame,
        componentId: String,
    },
}

impl Node {
    pub fn common(&self) -> &NodeCommon {
        match self {
            Node::DOCUMENT(node) => node,
            Node::CANVAS { node, .. } => node,
            Node::FRAME(Frame { node, .. }) => node,
            Node::GROUP(Frame { node, .. }) => node,
            Node::VECTOR(VectorNode { node, .. }) => node,
            Node::BOOLEAN_OPERATION { vector: VectorNode { node, .. }, .. } => node,
            Node::STAR(VectorNode { node, .. }) => node,
            Node::LINE(VectorNode { node, .. }) => node,
            Node::ELLIPSE(VectorNode { node, .. }) => node,
            Node::REGULAR_POLYGON(VectorNode { node, .. }) => node,
            Node::RECTANGLE { vector: VectorNode { node, .. }, .. } => node,
            Node::TEXT { vector: VectorNode { node, .. }, .. } => node,
            Node::SLICE { node, .. } => node,
            Node::COMPONENT(Frame { node, .. }) => node,
            Node::INSTANCE { frame: Frame { node, .. }, .. } => node,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Style {
    pub key: String,
    pub name: String,
    pub description: String,
    pub styleType: String,
}
