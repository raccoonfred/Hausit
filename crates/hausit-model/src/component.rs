//! Loadable families: components such as furniture, casework, plumbing
//! fixtures and equipment, defined by parametric solids in a `.hfam` file
//! (JSON) and placed with the Component tool.
//!
//! A family lists its parameters, its types (named sets of parameter
//! values) and its solids. Every dimension of a solid is an expression
//! over the parameters, such as `"Width / 2 - 25"`, so one family covers
//! every size. Families are stored in the project once loaded, as Revit
//! does.

use crate::category::Category;
use crate::document::{Document, ModelError};
use crate::element::Component;
use crate::family::TypeParams;
use crate::geometry::{Material, Part, Prism, Shape};
use crate::params::ParamValue;
use hausit_core::{DVec2, ElementId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EXTENSION: &str = "hfam";

/// A number, or an expression over the family's parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Expr {
    Number(f64),
    Text(String),
}

impl Expr {
    pub fn eval(&self, vars: &Vars) -> Result<f64, String> {
        match self {
            Expr::Number(v) => Ok(*v),
            Expr::Text(t) => eval(t, vars),
        }
    }
}

impl Expr {
    /// What the family editor shows for the expression.
    pub fn to_text(&self) -> String {
        match self {
            Expr::Number(v) => format_number(*v),
            Expr::Text(t) => t.clone(),
        }
    }

    /// The expression typed in the family editor: a plain number when it
    /// reads back the same, otherwise the text as typed.
    pub fn parse(text: &str) -> Expr {
        let t = text.trim();
        match t.parse::<f64>() {
            Ok(v) if v.is_finite() && format_number(v) == t => Expr::Number(v),
            _ => Expr::Text(t.to_string()),
        }
    }

    /// Renames a parameter wherever the expression uses it.
    pub fn rename(&mut self, old: &str, new: &str) {
        if let Expr::Text(t) = self {
            *t = rename_in(t, old, new);
        }
    }

    pub fn uses(&self, name: &str) -> bool {
        match self {
            Expr::Text(t) => names_in(t).iter().any(|n| n == name),
            Expr::Number(_) => false,
        }
    }
}

/// A number as the family editor writes it: no trailing zeros.
pub fn format_number(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// The parameter names an expression uses, with their byte ranges.
fn name_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let idx: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < idx.len() {
        let c = idx[i].1;
        if c.is_ascii_digit() || c == '.' {
            while i < idx.len() && (idx[i].1.is_ascii_digit() || idx[i].1 == '.') {
                i += 1;
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = idx[i].0;
            let mut end = start;
            while i < idx.len() && (idx[i].1.is_alphanumeric() || idx[i].1 == '_' || idx[i].1 == ' ') {
                if idx[i].1 != ' ' {
                    end = idx[i].0 + idx[i].1.len_utf8();
                }
                i += 1;
            }
            // A name followed by "(" is a function.
            let rest = text[end..].trim_start();
            if !rest.starts_with('(') {
                out.push((start, end));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn names_in(text: &str) -> Vec<String> {
    name_spans(text).into_iter().map(|(a, b)| text[a..b].to_string()).collect()
}

fn rename_in(text: &str, old: &str, new: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (a, b) in name_spans(text) {
        if &text[a..b] == old {
            out.push_str(&text[last..a]);
            out.push_str(new);
            last = b;
        }
    }
    out.push_str(&text[last..]);
    out
}

impl From<f64> for Expr {
    fn from(v: f64) -> Self {
        Expr::Number(v)
    }
}

/// The kind of a family parameter, as in Revit's parameter types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamType {
    Length,
    Integer,
    Number,
    Angle,
    #[serde(rename = "Yes/No")]
    YesNo,
    Text,
    Material,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyParam {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ParamType,
    pub default: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyTypeDef {
    pub name: String,
    #[serde(default)]
    pub values: BTreeMap<String, serde_json::Value>,
}

/// One solid of a family, in family coordinates: x to the right, y to the
/// back (the family faces -y), z up from the insertion point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape")]
pub enum Solid {
    /// An axis-aligned box from `min` to `max`.
    Box {
        min: [Expr; 3],
        max: [Expr; 3],
        #[serde(flatten)]
        common: SolidCommon,
    },
    /// A plan profile extruded from `z[0]` to `z[1]`.
    Extrusion {
        profile: Vec<[Expr; 2]>,
        z: [Expr; 2],
        #[serde(flatten)]
        common: SolidCommon,
    },
    /// A vertical cylinder.
    Cylinder {
        center: [Expr; 2],
        radius: Expr,
        z: [Expr; 2],
        #[serde(flatten)]
        common: SolidCommon,
    },
}

/// What every solid can have: a material, a visibility condition, and
/// repetition.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SolidCommon {
    /// A material name, or the name of a Material parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Shown only when this evaluates to non-zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<Expr>,
    /// Copies of the solid: `count` in all, each `step` from the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeat>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Repeat {
    pub count: Expr,
    pub step: [Expr; 3],
}

/// A symbolic line shown in plan instead of the solids' outlines, as
/// Revit's furniture and fixtures draw themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape")]
pub enum PlanSymbol {
    Line {
        from: [Expr; 2],
        to: [Expr; 2],
    },
    Rectangle {
        min: [Expr; 2],
        max: [Expr; 2],
    },
    Polyline {
        points: Vec<[Expr; 2]>,
        #[serde(default)]
        closed: bool,
    },
    Circle {
        center: [Expr; 2],
        radius: Expr,
    },
    Arc {
        center: [Expr; 2],
        radius: Expr,
        start: Expr,
        end: Expr,
    },
}

/// A loadable family.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyDef {
    pub name: String,
    pub category: Category,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<FamilyParam>,
    pub types: Vec<FamilyTypeDef>,
    pub solids: Vec<Solid>,
    #[serde(default)]
    pub plan: Vec<PlanSymbol>,
}

/// Parameter values by name, as numbers (lengths in mm, angles in
/// degrees, Yes/No as 1 or 0).
pub type Vars = BTreeMap<String, f64>;

impl FamilyDef {
    pub fn from_json(text: &str) -> Result<FamilyDef, String> {
        let def: FamilyDef = serde_json::from_str(text).map_err(|e| e.to_string())?;
        def.check()?;
        Ok(def)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Doors and windows: placed in a wall, which they cut.
    pub fn is_hosted(&self) -> bool {
        self.category.is_hosted()
    }

    /// Checks the family can be used: it has types, its category takes
    /// loadable families, hosted families size their opening, and every
    /// type's solids evaluate.
    pub fn check(&self) -> Result<(), String> {
        if !Category::COMPONENTS.contains(&self.category) && !self.is_hosted() {
            return Err(format!("{} cannot be loaded as a family", self.category.name()));
        }
        if self.types.is_empty() {
            return Err("the family has no types".into());
        }
        if self.is_hosted() {
            for need in ["Width", "Height"] {
                if !self.parameters.iter().any(|p| p.name == need && p.kind == ParamType::Length) {
                    return Err(format!("door and window families need a \"{need}\" length parameter"));
                }
            }
        }
        for t in &self.types {
            let params = self.type_params(t);
            let mut vars = vars_of(&params);
            if self.is_hosted() {
                vars.insert(WALL_THICKNESS.into(), 200.0);
            }
            for s in &self.solids {
                solid_prisms(s, &vars).map_err(|e| format!("type \"{}\": {e}", t.name))?;
            }
            for p in &self.plan {
                plan_lines(p, &vars).map_err(|e| format!("type \"{}\", plan symbol: {e}", t.name))?;
            }
        }
        Ok(())
    }

    /// The parameter values of one of the family's types.
    pub fn type_params(&self, t: &FamilyTypeDef) -> TypeParams {
        let mut p = TypeParams::new();
        for fp in &self.parameters {
            let v = t.values.get(&fp.name).unwrap_or(&fp.default);
            p.insert(fp.name.clone(), param_value(fp.kind, v));
        }
        p.entry("Type Mark".into()).or_insert(ParamValue::Text(String::new()));
        p.entry("Description".into()).or_insert(ParamValue::Text(self.description.clone()));
        p
    }
}

// ----- editing (the family editor) --------------------------------------------

impl ParamType {
    pub const ALL: [ParamType; 7] = [
        ParamType::Length,
        ParamType::Integer,
        ParamType::Number,
        ParamType::Angle,
        ParamType::YesNo,
        ParamType::Text,
        ParamType::Material,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ParamType::Length => "Length",
            ParamType::Integer => "Integer",
            ParamType::Number => "Number",
            ParamType::Angle => "Angle",
            ParamType::YesNo => "Yes/No",
            ParamType::Text => "Text",
            ParamType::Material => "Material",
        }
    }

    /// A starting value for a new parameter of this kind.
    pub fn default_value(self) -> serde_json::Value {
        match self {
            ParamType::Length => 100.0.into(),
            ParamType::Integer | ParamType::Number | ParamType::Angle => 0.into(),
            ParamType::YesNo => true.into(),
            ParamType::Text => "".into(),
            ParamType::Material => "Wood - Oak".into(),
        }
    }

    /// `v` converted to this kind, as far as it goes.
    pub fn convert(self, v: &serde_json::Value) -> serde_json::Value {
        let num = v.as_f64().or_else(|| v.as_bool().map(|b| f64::from(u8::from(b))));
        match (self, num) {
            (ParamType::Length | ParamType::Number | ParamType::Angle, Some(n)) => n.into(),
            (ParamType::Integer, Some(n)) => (n.round() as i64).into(),
            (ParamType::YesNo, Some(n)) => (n != 0.0).into(),
            (ParamType::Text, _) => v
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| num.map(format_number).unwrap_or_default())
                .into(),
            _ => self.default_value(),
        }
    }
}

impl Solid {
    pub fn common(&self) -> &SolidCommon {
        match self {
            Solid::Box { common, .. } | Solid::Extrusion { common, .. } | Solid::Cylinder { common, .. } => common,
        }
    }

    pub fn common_mut(&mut self) -> &mut SolidCommon {
        match self {
            Solid::Box { common, .. } | Solid::Extrusion { common, .. } | Solid::Cylinder { common, .. } => common,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Solid::Box { .. } => "Box",
            Solid::Extrusion { .. } => "Extrusion",
            Solid::Cylinder { .. } => "Cylinder",
        }
    }

    /// Every expression of the solid.
    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        let (mut v, common): (Vec<&mut Expr>, &mut SolidCommon) = match self {
            Solid::Box { min, max, common } => (min.iter_mut().chain(max.iter_mut()).collect(), common),
            Solid::Extrusion { profile, z, common } => (profile.iter_mut().flatten().chain(z.iter_mut()).collect(), common),
            Solid::Cylinder { center, radius, z, common } => (
                center.iter_mut().chain(std::iter::once(radius)).chain(z.iter_mut()).collect(),
                common,
            ),
        };
        if let Some(e) = &mut common.visible {
            v.push(e);
        }
        if let Some(r) = &mut common.repeat {
            v.push(&mut r.count);
            v.extend(r.step.iter_mut());
        }
        v
    }
}

impl PlanSymbol {
    pub fn kind(&self) -> &'static str {
        match self {
            PlanSymbol::Line { .. } => "Line",
            PlanSymbol::Rectangle { .. } => "Rectangle",
            PlanSymbol::Polyline { .. } => "Polyline",
            PlanSymbol::Circle { .. } => "Circle",
            PlanSymbol::Arc { .. } => "Arc",
        }
    }

    pub fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            PlanSymbol::Line { from, to } => from.iter_mut().chain(to.iter_mut()).collect(),
            PlanSymbol::Rectangle { min, max } => min.iter_mut().chain(max.iter_mut()).collect(),
            PlanSymbol::Polyline { points, .. } => points.iter_mut().flatten().collect(),
            PlanSymbol::Circle { center, radius } => center.iter_mut().chain(std::iter::once(radius)).collect(),
            PlanSymbol::Arc {
                center,
                radius,
                start,
                end,
            } => center.iter_mut().chain([radius, start, end]).collect(),
        }
    }
}

impl FamilyDef {
    /// A new family to start from in the family editor: a door or window
    /// with a frame and leaf or glass, or a box for other categories.
    pub fn template(category: Category) -> FamilyDef {
        let from_library = match category {
            Category::Doors => Some("Door - Single Flush"),
            Category::Windows => Some("Window - Fixed"),
            _ => None,
        };
        if let Some(mut def) = from_library.and_then(crate::library::family) {
            def.name = format!("New {}", category.singular());
            def.description = String::new();
            def.types.truncate(1);
            return def;
        }
        let p = |name: &str, kind: ParamType, default: serde_json::Value| FamilyParam {
            name: name.into(),
            kind,
            default,
        };
        let t = |s: &str| Expr::Text(s.into());
        FamilyDef {
            name: format!("New {}", category.singular()),
            category,
            description: String::new(),
            parameters: vec![
                p("Width", ParamType::Length, 600.0.into()),
                p("Depth", ParamType::Length, 600.0.into()),
                p("Height", ParamType::Length, 900.0.into()),
                p("Material", ParamType::Material, "Wood - Oak".into()),
            ],
            types: vec![FamilyTypeDef {
                name: "Type 1".into(),
                values: BTreeMap::new(),
            }],
            solids: vec![Solid::Box {
                min: [t("-Width / 2"), t("-Depth"), Expr::Number(0.0)],
                max: [t("Width / 2"), Expr::Number(0.0), t("Height")],
                common: SolidCommon {
                    material: Some("Material".into()),
                    ..Default::default()
                },
            }],
            plan: vec![],
        }
    }

    /// The values a type gives its expressions; hosted families also see
    /// a 200 mm wall.
    pub fn type_vars(&self, t: usize) -> Vars {
        let mut vars = self.types.get(t).map(|t| vars_of(&self.type_params(t))).unwrap_or_default();
        if self.is_hosted() {
            vars.insert(WALL_THICKNESS.into(), 200.0);
        }
        vars
    }

    /// Whether any solid or plan symbol uses the parameter.
    pub fn uses_parameter(&mut self, name: &str) -> bool {
        let mut used = false;
        for s in &mut self.solids {
            used |= s.common().material.as_deref() == Some(name);
            used |= s.exprs_mut().iter().any(|e| e.uses(name));
        }
        for p in &mut self.plan {
            used |= p.exprs_mut().iter().any(|e| e.uses(name));
        }
        used
    }

    /// Renames a parameter, in its types' values and in every formula
    /// that uses it.
    pub fn rename_parameter(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if new.is_empty() {
            return Err("a parameter needs a name".into());
        }
        if old == new {
            return Ok(());
        }
        if self.parameters.iter().any(|p| p.name == new) || new == WALL_THICKNESS {
            return Err(format!("there is already a parameter named \"{new}\""));
        }
        if !new.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
            || !new.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
        {
            return Err("parameter names start with a letter and use letters, digits, spaces and _".into());
        }
        let Some(p) = self.parameters.iter_mut().find(|p| p.name == old) else {
            return Err(format!("no parameter named \"{old}\""));
        };
        p.name = new.to_string();
        for t in &mut self.types {
            if let Some(v) = t.values.remove(old) {
                t.values.insert(new.to_string(), v);
            }
        }
        for s in &mut self.solids {
            for e in s.exprs_mut() {
                e.rename(old, new);
            }
            let c = s.common_mut();
            if c.material.as_deref() == Some(old) {
                c.material = Some(new.to_string());
            }
        }
        for p in &mut self.plan {
            for e in p.exprs_mut() {
                e.rename(old, new);
            }
        }
        Ok(())
    }

    /// Removes a parameter and its values from every type.
    pub fn remove_parameter(&mut self, name: &str) {
        self.parameters.retain(|p| p.name != name);
        for t in &mut self.types {
            t.values.remove(name);
        }
    }

    /// The value a type gives a parameter (its own, or the default).
    pub fn value(&self, t: usize, name: &str) -> Option<&serde_json::Value> {
        let p = self.parameters.iter().find(|p| p.name == name)?;
        Some(self.types.get(t).and_then(|t| t.values.get(name)).unwrap_or(&p.default))
    }

    /// Sets a type's value for a parameter.
    pub fn set_value(&mut self, t: usize, name: &str, v: serde_json::Value) {
        if let Some(t) = self.types.get_mut(t) {
            t.values.insert(name.to_string(), v);
        }
    }

    /// A free name like `base`, `base 2`, `base 3`...
    pub fn unique_type_name(&self, base: &str) -> String {
        let mut name = base.to_string();
        let mut i = 2;
        while self.types.iter().any(|t| t.name == name) {
            name = format!("{base} {i}");
            i += 1;
        }
        name
    }
}

fn param_value(kind: ParamType, v: &serde_json::Value) -> ParamValue {
    let num = v.as_f64().unwrap_or(0.0);
    match kind {
        ParamType::Length => ParamValue::Length(num),
        ParamType::Integer => ParamValue::Integer(num.round() as i64),
        ParamType::Number => ParamValue::Number(num),
        ParamType::Angle => ParamValue::Angle(num),
        ParamType::YesNo => ParamValue::Bool(v.as_bool().unwrap_or(num != 0.0)),
        ParamType::Text | ParamType::Material => ParamValue::Text(v.as_str().unwrap_or_default().to_string()),
    }
}

/// Numeric values of a type's parameters, for expressions.
pub fn vars_of(params: &TypeParams) -> Vars {
    params
        .iter()
        .filter_map(|(k, v)| {
            let n = match v {
                ParamValue::Length(x) | ParamValue::Number(x) | ParamValue::Angle(x) => *x,
                ParamValue::Area(x) | ParamValue::Volume(x) => *x,
                ParamValue::Integer(i) => *i as f64,
                ParamValue::Bool(b) => f64::from(u8::from(*b)),
                _ => return None,
            };
            Some((k.clone(), n))
        })
        .collect()
}

// ----- expressions -----------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let t: String = chars[start..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number \"{t}\""))?));
        } else if c.is_alphabetic() || c == '_' {
            // Names may contain spaces ("Seat Height"), as Revit's do.
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == ' ') {
                i += 1;
            }
            let t: String = chars[start..i].iter().collect();
            out.push(Tok::Name(t.trim_end().to_string()));
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let op = match two.as_str() {
                ">=" => Some(">="),
                "<=" => Some("<="),
                "==" => Some("=="),
                "!=" => Some("!="),
                _ => None,
            };
            if let Some(op) = op {
                out.push(Tok::Op(op));
                i += 2;
                continue;
            }
            out.push(match c {
                '+' => Tok::Op("+"),
                '-' => Tok::Op("-"),
                '*' => Tok::Op("*"),
                '/' => Tok::Op("/"),
                '>' => Tok::Op(">"),
                '<' => Tok::Op("<"),
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ',' => Tok::Comma,
                _ => return Err(format!("unexpected \"{c}\"")),
            });
            i += 1;
        }
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    vars: &'a Vars,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn comparison(&mut self) -> Result<f64, String> {
        let a = self.sum()?;
        if let Some(Tok::Op(op @ (">" | "<" | ">=" | "<=" | "==" | "!="))) = self.peek().cloned() {
            self.pos += 1;
            let b = self.sum()?;
            let r = match op {
                ">" => a > b,
                "<" => a < b,
                ">=" => a >= b,
                "<=" => a <= b,
                "==" => (a - b).abs() < 1e-9,
                _ => (a - b).abs() >= 1e-9,
            };
            return Ok(f64::from(u8::from(r)));
        }
        Ok(a)
    }

    fn sum(&mut self) -> Result<f64, String> {
        let mut v = self.product()?;
        while let Some(Tok::Op(op @ ("+" | "-"))) = self.peek().cloned() {
            self.pos += 1;
            let b = self.product()?;
            v = if op == "+" { v + b } else { v - b };
        }
        Ok(v)
    }

    fn product(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        while let Some(Tok::Op(op @ ("*" | "/"))) = self.peek().cloned() {
            self.pos += 1;
            let b = self.unary()?;
            v = if op == "*" {
                v * b
            } else if b.abs() < 1e-12 {
                return Err("division by zero".into());
            } else {
                v / b
            };
        }
        Ok(v)
    }

    fn unary(&mut self) -> Result<f64, String> {
        if let Some(Tok::Op("-")) = self.peek() {
            self.pos += 1;
            return Ok(-self.unary()?);
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<f64, String> {
        match self.next() {
            Some(Tok::Num(v)) => Ok(v),
            Some(Tok::LParen) => {
                let v = self.comparison()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(v),
                    _ => Err("missing \")\"".into()),
                }
            }
            Some(Tok::Name(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.pos += 1;
                    let mut args = vec![self.comparison()?];
                    loop {
                        match self.next() {
                            Some(Tok::Comma) => args.push(self.comparison()?),
                            Some(Tok::RParen) => break,
                            _ => return Err(format!("missing \")\" after {name}(")),
                        }
                    }
                    return call(&name, &args);
                }
                self.vars.get(&name).copied().ok_or_else(|| format!("unknown parameter \"{name}\""))
            }
            Some(t) => Err(format!("unexpected {t:?}")),
            None => Err("the expression ends too soon".into()),
        }
    }
}

fn call(name: &str, a: &[f64]) -> Result<f64, String> {
    let need = |n: usize| {
        if a.len() == n {
            Ok(())
        } else {
            Err(format!("{name}() takes {n} values"))
        }
    };
    Ok(match name.to_ascii_lowercase().as_str() {
        "min" if !a.is_empty() => a.iter().copied().fold(f64::INFINITY, f64::min),
        "max" if !a.is_empty() => a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "abs" => {
            need(1)?;
            a[0].abs()
        }
        "sqrt" => {
            need(1)?;
            a[0].max(0.0).sqrt()
        }
        "round" => {
            need(1)?;
            a[0].round()
        }
        "floor" => {
            need(1)?;
            a[0].floor()
        }
        "if" => {
            need(3)?;
            if a[0] != 0.0 {
                a[1]
            } else {
                a[2]
            }
        }
        _ => return Err(format!("unknown function {name}()")),
    })
}

/// Evaluates an expression such as `"(Width - 2 * Frame) / 2"`.
pub fn eval(text: &str, vars: &Vars) -> Result<f64, String> {
    let mut p = Parser {
        toks: tokenize(text)?,
        pos: 0,
        vars,
    };
    let v = p.comparison()?;
    if p.pos < p.toks.len() {
        return Err(format!("unexpected {:?} in \"{text}\"", p.toks[p.pos]));
    }
    if !v.is_finite() {
        return Err(format!("\"{text}\" is not a finite number"));
    }
    Ok(v)
}

// ----- geometry --------------------------------------------------------------

fn pt(e: &[Expr; 2], vars: &Vars) -> Result<DVec2, String> {
    Ok(DVec2::new(e[0].eval(vars)?, e[1].eval(vars)?))
}

/// A solid's prisms (one per copy) in family coordinates, with its
/// material, or none when it is hidden or empty.
pub fn solid_prisms(s: &Solid, vars: &Vars) -> Result<(Vec<Prism>, Option<String>), String> {
    let common = match s {
        Solid::Box { common, .. } | Solid::Extrusion { common, .. } | Solid::Cylinder { common, .. } => common,
    };
    if let Some(v) = &common.visible {
        if v.eval(vars)? == 0.0 {
            return Ok((vec![], None));
        }
    }
    let base = match s {
        Solid::Box { min, max, .. } => {
            let [x0, y0, z0] = [&min[0], &min[1], &min[2]].map(|e| e.eval(vars));
            let [x1, y1, z1] = [&max[0], &max[1], &max[2]].map(|e| e.eval(vars));
            let (x0, y0, z0, x1, y1, z1) = (x0?, y0?, z0?, x1?, y1?, z1?);
            let (xa, xb) = (x0.min(x1), x0.max(x1));
            let (ya, yb) = (y0.min(y1), y0.max(y1));
            if xb - xa < 1e-6 || yb - ya < 1e-6 {
                None
            } else {
                Some(Prism::new(
                    vec![DVec2::new(xa, ya), DVec2::new(xb, ya), DVec2::new(xb, yb), DVec2::new(xa, yb)],
                    z0,
                    z1,
                ))
            }
        }
        Solid::Extrusion { profile, z, .. } => {
            let pts = profile.iter().map(|p| pt(p, vars)).collect::<Result<Vec<_>, _>>()?;
            let (z0, z1) = (z[0].eval(vars)?, z[1].eval(vars)?);
            (pts.len() >= 3).then(|| Prism::new(pts, z0, z1))
        }
        Solid::Cylinder { center, radius, z, .. } => {
            let c = pt(center, vars)?;
            let r = radius.eval(vars)?;
            let n = 20;
            let pts: Vec<DVec2> = (0..n)
                .map(|i| c + DVec2::from_angle(std::f64::consts::TAU * i as f64 / n as f64) * r)
                .collect();
            let (z0, z1) = (z[0].eval(vars)?, z[1].eval(vars)?);
            (r > 1e-6).then(|| Prism::new(pts, z0, z1))
        }
    };
    let Some(base) = base.filter(|p| p.z1 - p.z0 > 1e-6) else {
        return Ok((vec![], None));
    };
    let mut prisms = vec![base.clone()];
    if let Some(rep) = &common.repeat {
        let count = rep.count.eval(vars)?.round().clamp(0.0, 500.0) as usize;
        let (dx, dy, dz) = (rep.step[0].eval(vars)?, rep.step[1].eval(vars)?, rep.step[2].eval(vars)?);
        prisms = (0..count)
            .map(|i| {
                let k = i as f64;
                Prism {
                    footprint: base.footprint.iter().map(|p| *p + DVec2::new(dx, dy) * k).collect(),
                    z0: base.z0 + dz * k,
                    z1: base.z1 + dz * k,
                }
            })
            .collect();
    }
    Ok((prisms, common.material.clone()))
}

/// Plan lines of a symbol, in family coordinates.
pub fn plan_lines(p: &PlanSymbol, vars: &Vars) -> Result<Vec<(DVec2, DVec2)>, String> {
    let poly = |pts: Vec<DVec2>, closed: bool| {
        let n = pts.len();
        let m = if closed { n } else { n.saturating_sub(1) };
        (0..m).map(|i| (pts[i], pts[(i + 1) % n])).collect::<Vec<_>>()
    };
    let arc = |c: DVec2, r: f64, a0: f64, a1: f64| {
        let n = (((a1 - a0).abs() / 10.0).ceil() as usize).max(2);
        let pts: Vec<DVec2> = (0..=n)
            .map(|i| c + DVec2::from_angle((a0 + (a1 - a0) * i as f64 / n as f64).to_radians()) * r)
            .collect();
        poly(pts, false)
    };
    Ok(match p {
        PlanSymbol::Line { from, to } => vec![(pt(from, vars)?, pt(to, vars)?)],
        PlanSymbol::Rectangle { min, max } => {
            let (a, b) = (pt(min, vars)?, pt(max, vars)?);
            poly(vec![a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)], true)
        }
        PlanSymbol::Polyline { points, closed } => {
            let pts = points.iter().map(|q| pt(q, vars)).collect::<Result<Vec<_>, _>>()?;
            poly(pts, *closed)
        }
        PlanSymbol::Circle { center, radius } => arc(pt(center, vars)?, radius.eval(vars)?, 0.0, 360.0),
        PlanSymbol::Arc {
            center,
            radius,
            start,
            end,
        } => arc(pt(center, vars)?, radius.eval(vars)?, start.eval(vars)?, end.eval(vars)?),
    })
}

/// Where a family's coordinates land in the model: the plan transform
/// (with optional mirroring) and the base height.
pub struct Placement {
    pub origin: DVec2,
    pub dir: DVec2,
    pub z: f64,
    /// Mirror the family's x (a door's hand) and y (its facing).
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Placement {
    pub fn of(doc: &Document, c: &Component) -> Placement {
        Placement {
            origin: c.point,
            dir: DVec2::from_angle(c.rotation.to_radians()),
            z: doc.level_elevation(c.level) + c.offset,
            flip_x: false,
            flip_y: false,
        }
    }

    pub fn apply(&self, p: DVec2) -> DVec2 {
        let q = DVec2::new(if self.flip_x { -p.x } else { p.x }, if self.flip_y { -p.y } else { p.y });
        self.origin + self.dir.rotate(q)
    }
}

/// Name of the variable hosted families can use for their wall's
/// thickness.
pub const WALL_THICKNESS: &str = "Wall Thickness";

/// The family and the parameter values behind an element's type.
fn family_and_vars(doc: &Document, type_id: ElementId) -> Option<(&FamilyDef, Vars)> {
    let t = doc.family_type(type_id)?;
    let def = doc.family_def(t.category, &t.family)?;
    Some((def, vars_of(&t.params)))
}

/// A family type's solids, placed in the model.
fn family_parts(doc: &Document, type_id: ElementId, def: &FamilyDef, vars: &Vars, place: &Placement) -> Vec<Part> {
    let t = doc.family_type(type_id);
    let mut parts = Vec::new();
    for s in &def.solids {
        let Ok((prisms, material)) = solid_prisms(s, vars) else { continue };
        // A material name, or a Material parameter naming one.
        let material = material.map(|m| match t.and_then(|t| t.params.get(&m)) {
            Some(ParamValue::Text(name)) => name.clone(),
            _ => m,
        });
        let kind = match material.as_deref() {
            Some(m) if m.to_ascii_lowercase().contains("glass") => Material::Glass,
            _ => Material::Default,
        };
        for p in prisms {
            parts.push(Part {
                material: kind,
                material_name: material.clone(),
                shape: Shape::Prism(Prism::new(
                    p.footprint.iter().map(|q| place.apply(*q)).collect(),
                    place.z + p.z0,
                    place.z + p.z1,
                )),
            });
        }
    }
    parts
}

/// A family's plan symbol, placed in the model; `None` when it has none.
fn family_plan(def: &FamilyDef, vars: &Vars, place: &Placement) -> Option<Vec<(DVec2, DVec2)>> {
    if def.plan.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for p in &def.plan {
        for (a, b) in plan_lines(p, vars).ok()? {
            lines.push((place.apply(a), place.apply(b)));
        }
    }
    Some(lines)
}

/// The solids of a component, in model coordinates.
pub fn component_parts(doc: &Document, c: &Component) -> Vec<Part> {
    let Some((def, vars)) = family_and_vars(doc, c.type_id) else {
        return vec![];
    };
    family_parts(doc, c.type_id, def, &vars, &Placement::of(doc, c))
}

/// The plan symbol of a component in model coordinates, or `None` when
/// its family has none (plans then draw the solids' outlines).
pub fn component_plan(doc: &Document, c: &Component) -> Option<Vec<(DVec2, DVec2)>> {
    let (def, vars) = family_and_vars(doc, c.type_id)?;
    family_plan(def, &vars, &Placement::of(doc, c))
}

/// A door or window's family, its values (with the host wall's
/// thickness) and its placement: centred on the opening, x along the
/// wall, y towards the wall's exterior, z from the bottom of the opening.
fn hosted(doc: &Document, id: ElementId) -> Option<(&FamilyDef, Vars, Placement)> {
    let o = doc.element(id)?.as_opening()?;
    let (def, mut vars) = family_and_vars(doc, o.type_id)?;
    let f = crate::geometry::opening_frame(doc, id)?;
    vars.insert(WALL_THICKNESS.into(), f.wall_thickness);
    let place = Placement {
        origin: f.center,
        dir: f.dir,
        z: f.z0,
        flip_x: o.flip_hand,
        flip_y: o.flip_facing,
    };
    Some((def, vars, place))
}

/// The solids of a door or window from its family, or `None` for the
/// built-in types without one.
pub fn hosted_parts(doc: &Document, id: ElementId) -> Option<Vec<Part>> {
    let (def, vars, place) = hosted(doc, id)?;
    let o = doc.element(id)?.as_opening()?;
    Some(family_parts(doc, o.type_id, def, &vars, &place))
}

/// The plan symbol of a door or window from its family, if it has one.
pub fn hosted_plan(doc: &Document, id: ElementId) -> Option<Vec<(DVec2, DVec2)>> {
    let (def, vars, place) = hosted(doc, id)?;
    family_plan(def, &vars, &place)
}

impl Document {
    pub fn family_defs(&self) -> impl Iterator<Item = &FamilyDef> {
        self.families.iter()
    }

    pub fn family_def(&self, category: Category, name: &str) -> Option<&FamilyDef> {
        self.families.iter().find(|f| f.category == category && f.name == name)
    }

    /// Loads a family into the project: adds it (or replaces an older
    /// version of it) and creates the types the project does not have
    /// yet. Returns the new types.
    pub fn load_family(&mut self, def: FamilyDef) -> Result<Vec<ElementId>, ModelError> {
        self.load_family_with(def, false)
    }

    /// Loads a family; with `overwrite`, types already in the project also
    /// take the family's values (Revit's "Overwrite the existing version
    /// and its parameter values"), as when reloading from the family
    /// editor.
    pub fn load_family_with(&mut self, def: FamilyDef, overwrite: bool) -> Result<Vec<ElementId>, ModelError> {
        def.check().map_err(ModelError::Rejected)?;
        // Bring in the library materials the family uses.
        let mut names: Vec<String> = def
            .solids
            .iter()
            .filter_map(|s| match s {
                Solid::Box { common, .. } | Solid::Extrusion { common, .. } | Solid::Cylinder { common, .. } => common.material.clone(),
            })
            .collect();
        for t in &def.types {
            for p in def.parameters.iter().filter(|p| p.kind == ParamType::Material) {
                if let Some(v) = t.values.get(&p.name).unwrap_or(&p.default).as_str() {
                    names.push(v.to_string());
                }
            }
        }
        for name in names {
            if self.material(&name).is_none() {
                if let Some(m) = crate::materials::default_materials().into_iter().find(|m| m.name == name) {
                    self.materials.push(m);
                }
            }
        }
        let mut added = Vec::new();
        for t in &def.types {
            let existing = self
                .types_of(def.category)
                .find(|ft| ft.family == def.name && ft.name == t.name)
                .map(|ft| ft.id);
            match existing {
                Some(id) if overwrite => {
                    if let Some(ft) = self.family_type_mut(id) {
                        for (k, v) in def.type_params(t) {
                            ft.params.insert(k, v);
                        }
                    }
                }
                Some(_) => {}
                None => added.push(self.add_type(def.category, &def.name, &t.name, def.type_params(t))),
            }
        }
        match self.families.iter_mut().find(|f| f.category == def.category && f.name == def.name) {
            Some(old) => *old = def,
            None => self.families.push(def),
        }
        self.touch();
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, f64)]) -> Vars {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn expressions() {
        let v = vars(&[("Width", 900.0), ("Seat Height", 450.0), ("Doors", 1.0)]);
        assert_eq!(eval("Width / 2 - 25", &v).unwrap(), 425.0);
        assert_eq!(eval("-(Width - 2 * 50) / 4", &v).unwrap(), -200.0);
        assert_eq!(eval("Seat Height + 10", &v).unwrap(), 460.0);
        assert_eq!(eval("max(Width, 1000, 3)", &v).unwrap(), 1000.0);
        assert_eq!(eval("if(Width > 800, 1, 0)", &v).unwrap(), 1.0);
        assert_eq!(eval("Doors == 1", &v).unwrap(), 1.0);
        assert!(eval("Height", &v).is_err());
        assert!(eval("Width +", &v).is_err());
        assert!(eval("Width / (Doors - 1)", &v).is_err());
    }

    #[test]
    fn renames_parameters_in_formulas() {
        let mut def = FamilyDef::template(Category::Furniture);
        assert!(def.uses_parameter("Width"));
        def.rename_parameter("Width", "Overall Width").unwrap();
        let Solid::Box { min, .. } = &def.solids[0] else { panic!() };
        assert_eq!(min[0], Expr::Text("-Overall Width / 2".into()));
        assert!(def.rename_parameter("Depth", "Height").is_err());
        assert!(def.check().is_ok());
        assert_eq!(rename_in("max(Width, Widths) + Width", "Width", "W"), "max(W, Widths) + W");
        assert_eq!(Expr::parse("12"), Expr::Number(12.0));
        assert_eq!(Expr::parse("12."), Expr::Text("12.".into()));
        assert_eq!(Expr::Number(-0.5).to_text(), "-0.5");
    }

    #[test]
    fn templates_check() {
        for c in [Category::Doors, Category::Windows].into_iter().chain(Category::COMPONENTS) {
            let def = FamilyDef::template(c);
            assert!(def.check().is_ok(), "{c:?}: {:?}", def.check());
        }
    }

    #[test]
    fn reload_overwrites_values() {
        let mut doc = crate::template::new_project();
        let mut def = FamilyDef::template(Category::Furniture);
        doc.load_family(def.clone()).unwrap();
        def.set_value(0, "Width", 1234.0.into());
        doc.load_family(def.clone()).unwrap();
        let t = doc.types_of(Category::Furniture).find(|t| t.family == def.name).unwrap();
        assert_eq!(t.params.get("Width"), Some(&ParamValue::Length(600.0)));
        doc.load_family_with(def.clone(), true).unwrap();
        let t = doc.types_of(Category::Furniture).find(|t| t.family == def.name).unwrap();
        assert_eq!(t.params.get("Width"), Some(&ParamValue::Length(1234.0)));
    }

    #[test]
    fn library_families_load_and_build() {
        for def in crate::library::families() {
            assert!(def.check().is_ok(), "{}: {:?}", def.name, def.check());
            for t in &def.types {
                let mut v = vars_of(&def.type_params(t));
                v.insert(WALL_THICKNESS.into(), 200.0);
                let n: usize = def.solids.iter().map(|s| solid_prisms(s, &v).unwrap().0.len()).sum();
                assert!(n > 0, "{} : {} has no solids", def.name, t.name);
            }
        }
    }
}
