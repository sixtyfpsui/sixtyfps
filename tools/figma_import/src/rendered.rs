/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use crate::figmatypes::{self, *};
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Write;

pub struct Document<'doc> {
    pub nodeHash: HashMap<&'doc str, &'doc figmatypes::Node>,
    //pub images: HashMap<String, Vec<u8>>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Indent(pub u32);

impl std::ops::SubAssign<u32> for Indent {
    fn sub_assign(&mut self, rhs: u32) {
        self.0 -= rhs;
    }
}

impl std::ops::AddAssign<u32> for Indent {
    fn add_assign(&mut self, rhs: u32) {
        self.0 += rhs;
    }
}

impl Display for Indent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for _ in 0..self.0 {
            write!(f, "    ")?
        }
        Ok(())
    }
}

#[derive(Default)]
struct Ctx {
    out: String,
    indent: Indent,
    offset: Vector,
}

impl Ctx {
    fn begin_element(
        &mut self,
        element: &str,
        node: &NodeCommon,
        absoluteBoundingBox: Option<&Rectangle>,
    ) -> std::fmt::Result {
        writeln!(
            self,
            "id_{} := {} {{ /* {} */",
            node.id.replace(":", "-").replace(";", "_"),
            element,
            node.name
        )?;
        self.indent += 1;
        if let Some(bb) = absoluteBoundingBox {
            writeln!(self, "width: {}px;", bb.width)?;
            writeln!(self, "height: {}px;", bb.height)?;
            writeln!(self, "x: {}px;", bb.x - self.offset.x)?;
            writeln!(self, "y: {}px;", bb.y - self.offset.y)?;
        }
        Ok(())
    }

    fn end_element(&mut self) -> std::fmt::Result {
        self.indent -= 1;
        writeln!(self, "}}")
    }
}

impl Write for Ctx {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.out.as_bytes().last() == Some(&b'\n') {
            write!(self.out, "{}", self.indent)?;
        }
        self.out.push_str(s);
        Ok(())
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8,
            (self.a * 255.0) as u8
        )
    }
}

pub fn render(
    _name: &str,
    node: &Node,
    background: Color,
    doc: &Document,
) -> Result<String, Box<dyn std::error::Error>> {
    /*= match node {
        Node::FRAME(Frame { absoluteBoundingBox, .. }) => absoluteBoundingBox,
        Node::GROUP(Frame { absoluteBoundingBox, .. }) => absoluteBoundingBox,
        Node::COMPONENT(Frame { absoluteBoundingBox, .. }) => absoluteBoundingBox,
        Node::INSTANCE {
            frame: Frame { absoluteBoundingBox, .. },
            ..
        } => absoluteBoundingBox,
        _ => return Err(super::Error("Rendering not a frame".into()).into())
    };*/
    let frame = match node {
        Node::FRAME(f) => f,
        Node::GROUP(f) => f,
        Node::COMPONENT(f) => f,
        //         Node::INSTANCE { frame } => frame,
        _ => return Err(super::Error("Rendering not a frame".into()).into()),
    };

    let mut ctx = Ctx::default();
    writeln!(ctx, "App := Window {{")?;
    ctx.indent += 1;
    writeln!(ctx, "background: {};", background)?;
    writeln!(ctx, "width: {}px;", frame.absoluteBoundingBox.width)?;
    writeln!(ctx, "height: {}px;", frame.absoluteBoundingBox.height)?;
    ctx.offset = frame.absoluteBoundingBox.origin();
    render_node(node, &mut ctx, doc)?;
    ctx.end_element()?;

    Ok(ctx.out)
}

fn render_frame(frame: &Frame, rc: &mut Ctx) -> Result<bool, Box<dyn std::error::Error>> {
    rc.begin_element("Rectangle", &frame.node, Some(&frame.absoluteBoundingBox))?;
    rc.offset = frame.absoluteBoundingBox.origin();
    let mut has_background = false;
    for p in frame.background.iter() {
        has_background |= handle_paint(p, rc, "background")?;
    }
    if !has_background && !frame.backgroundColor.is_transparent() {
        writeln!(rc, "background: {};", frame.backgroundColor)?;
    }
    if frame.clipsContent || frame.isMask {
        writeln!(rc, "Clip {{")?;
        rc.indent += 1;
    }
    Ok(frame.isMask)
}

fn render_vector(
    vector: &VectorNode,
    rc: &mut Ctx,
    _doc: &Document,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !vector.fillGeometry.is_empty() || !vector.strokeGeometry.is_empty() {
        for p in vector.fillGeometry.iter().chain(vector.strokeGeometry.iter()) {
            rc.begin_element("Path", &vector.node, Some(&vector.absoluteBoundingBox))?;
            writeln!(rc, "commands: \"{}\";", p.path)?;
            writeln!(rc, "fill-rule: {};", p.windingRule.to_ascii_lowercase())?;
            if vector.strokeWeight > 0. {
                writeln!(rc, "stroke-width: {}px;", vector.strokeWeight)?;
            }
            for p in vector.strokes.iter() {
                handle_paint(p, rc, "stroke")?;
            }
            for p in vector.fills.iter() {
                handle_paint(p, rc, "fill")?;
                if let Some(_imr) = &p.imageRef { /* */ }
            }
            rc.end_element()?;
            return Ok(false);
        }
    }

    for p in vector.fills.iter() {
        if let Some(color) = &p.color {
            if !color.is_transparent() {
                rc.begin_element("Rectangle", &vector.node, Some(&vector.absoluteBoundingBox))?;
                handle_paint(p, rc, "background")?;
                rc.end_element()?;
            }
        }
        if let Some(imr) = &p.imageRef {
            rc.begin_element("Image", &vector.node, Some(&vector.absoluteBoundingBox))?;
            writeln!(rc, "source: @image-url(\"images/{}\");", imr.escape_debug())?;
            rc.end_element()?;
        }
    }
    Ok(false)
}

fn render_text(
    text: &str,
    font: &TypeStyle,
    vector: &VectorNode,
    rc: &mut Ctx,
) -> Result<(), Box<dyn std::error::Error>> {
    rc.begin_element("Text", &vector.node, Some(&vector.absoluteBoundingBox))?;
    writeln!(rc, "text: \"{}\";", text.escape_debug())?;
    writeln!(rc, "font-family: \"{}\";", font.fontFamily)?;
    writeln!(rc, "font-size: {}px;", font.fontSize)?;
    writeln!(rc, "font-weight: {};", font.fontWeight)?;
    writeln!(rc, "horizontal-alignment: {};", font.textAlignHorizontal.to_ascii_lowercase())?;
    writeln!(rc, "vertical-alignment: {};", font.textAlignVertical.to_ascii_lowercase())?;
    writeln!(rc, "letter-spacing: {}px;", font.letterSpacing)?;
    for p in vector.fills.iter() {
        handle_paint(p, rc, "color")?;
    }
    rc.end_element()?;
    Ok(())
}

fn render_rectangle(
    vector: &VectorNode,
    cornerRadius: &Option<f32>,
    rc: &mut Ctx,
    _doc: &Document,
) -> Result<bool, Box<dyn std::error::Error>> {
    rc.begin_element("Rectangle", &vector.node, Some(&vector.absoluteBoundingBox))?;
    rc.offset = vector.absoluteBoundingBox.origin();
    if let Some(cornerRadius) = cornerRadius {
        // Note that figma rendering when the cornerRadius > min(height,width)/2 is different
        // than ours, so we adjust it there
        let min_edge = vector.absoluteBoundingBox.width.min(vector.absoluteBoundingBox.height);
        writeln!(rc, "border-radius: {}px;", cornerRadius.min(min_edge / 2.))?;
    }
    let mut has_border = false;
    for p in vector.strokes.iter() {
        has_border |= handle_paint(p, rc, "border-color")?;
    }
    if vector.strokeWeight > 0. && has_border {
        writeln!(rc, "border-width: {}px;", vector.strokeWeight)?;
    }
    for p in vector.fills.iter() {
        handle_paint(p, rc, "background")?;
        if let Some(imr) = &p.imageRef {
            writeln!(rc, "Image {{")?;
            writeln!(rc, "    width: 100%; height: 100%;")?;
            writeln!(rc, "    source: @image-url(\"images/{}\");", imr.escape_debug())?;
            match p.scaleMode.as_ref().map(|x| x.as_str()) {
                Some("FIT") => writeln!(rc, "    image-fit: contain;")?,
                _ => (),
            }
            writeln!(rc, "    }}")?;
        }
    }
    if vector.isMask {
        writeln!(rc, "Clip {{")?;
        rc.indent += 1;
    }
    Ok(vector.isMask)
}

fn render_line(
    vector: &VectorNode,
    rc: &mut Ctx,
    _doc: &Document,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bb = vector.absoluteBoundingBox;
    if bb.height == 0. {
        bb.y -= vector.strokeWeight;
        bb.height += vector.strokeWeight;
    }
    if bb.width == 0. {
        bb.x -= vector.strokeWeight;
        bb.width += vector.strokeWeight;
    }

    rc.begin_element("Rectangle", &vector.node, Some(&bb))?;
    for p in vector.strokes.iter() {
        handle_paint(p, rc, "background")?;
    }
    rc.end_element()?;
    Ok(())
}

fn render_node(
    node: &figmatypes::Node,
    rc: &mut Ctx,
    doc: &Document,
) -> Result<(), Box<dyn std::error::Error>> {
    let prev_ctx = (rc.indent, rc.offset);
    let is_mask = match node {
        Node::FRAME(f) => render_frame(f, rc)?,
        Node::GROUP(f) => render_frame(f, rc)?,
        Node::COMPONENT(f) => render_frame(f, rc)?,
        // Node::INSTANCE { frame } => frame,
        Node::VECTOR(vector) => render_vector(vector, rc, doc)?,
        Node::BOOLEAN_OPERATION { vector, .. } => render_vector(vector, rc, doc)?,
        Node::STAR(vector) => render_vector(vector, rc, doc)?,
        Node::LINE(vector) => {
            render_line(vector, rc, doc)?;
            false
        }
        Node::ELLIPSE(vector) => render_vector(vector, rc, doc)?,
        Node::REGULAR_POLYGON(vector) => render_vector(vector, rc, doc)?,
        Node::RECTANGLE { vector, cornerRadius, .. } => {
            render_rectangle(vector, cornerRadius, rc, doc)?
        }
        Node::TEXT { vector, characters, style, .. } => {
            render_text(characters, style, &vector, rc)?;
            false
        }
        _ => false,
    };

    for x in node.common().children.iter() {
        render_node(&x, rc, doc)?;
    }

    if is_mask {
        return Ok(());
    }

    while rc.indent != prev_ctx.0 {
        rc.indent -= 1;
        writeln!(rc, "}}")?;
    }
    rc.offset = prev_ctx.1;

    Ok(())
}

fn handle_paint(p: &Paint, rc: &mut Ctx, arg: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if !p.visible {
        return Ok(false);
    }
    let mut has_something = false;
    if !p.gradientStops.is_empty() {
        if p.r#type != "GRADIENT_LINEAR" {
            eprintln!("Warning: unsuported Paint type {:?}", p.r#type);
            return Ok(false);
        }
        let p1 = *p
            .gradientHandlePositions
            .get(0)
            .ok_or_else(|| "Gradient with missing 'gradientHandlePositions'".to_string())?;
        let p2 = *p
            .gradientHandlePositions
            .get(1)
            .ok_or_else(|| "Gradient with missing 'gradientHandlePositions'".to_string())?;
        let sub = p1 - p2;
        write!(rc, "{}: @linear-gradient({}deg", arg, -f32::atan2(sub.x, sub.y).to_degrees())?;
        for stop in &p.gradientStops {
            write!(rc, ", {} {}", stop.color, stop.position)?;
        }
        writeln!(rc, ");")?;
        has_something = true;
    } else if let Some(color) = &p.color {
        if !color.is_transparent() {
            writeln!(rc, "{}: {};", arg, color)?;
            has_something = true;
        }
    }
    Ok(has_something)
}
