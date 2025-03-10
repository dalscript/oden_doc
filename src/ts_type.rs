// Copyright 2020-2022 the Deno authors. All rights reserved. MIT license.

use crate::colors;
use crate::display::display_computed;
use crate::display::display_optional;
use crate::display::display_readonly;
use crate::display::SliceDisplayer;
use crate::interface::expr_to_name;
use crate::params::pat_to_param_def;
use crate::params::ts_fn_param_to_param_def;
use crate::swc_util::is_false;
use crate::ts_type_param::maybe_type_param_decl_to_type_param_defs;
use crate::ts_type_param::TsTypeParamDef;
use crate::ParamDef;

use deno_ast::swc::ast::*;
use deno_ast::swc::common::Span;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

impl From<&TsLitType> for TsTypeDef {
  fn from(other: &TsLitType) -> TsTypeDef {
    match &other.lit {
      TsLit::Number(num) => (TsTypeDef::number_literal(num)),
      TsLit::Str(str_) => (TsTypeDef::string_literal(str_)),
      TsLit::Tpl(tpl) => TsTypeDef::tpl_literal(&tpl.types, &tpl.quasis),
      TsLit::Bool(bool_) => (TsTypeDef::bool_literal(bool_)),
      TsLit::BigInt(bigint_) => (TsTypeDef::bigint_literal(bigint_)),
    }
  }
}

impl From<&TsArrayType> for TsTypeDef {
  fn from(other: &TsArrayType) -> TsTypeDef {
    let ts_type_def: TsTypeDef = (&*other.elem_type).into();

    TsTypeDef {
      array: Some(Box::new(ts_type_def)),
      kind: Some(TsTypeDefKind::Array),
      ..Default::default()
    }
  }
}

impl From<&TsTupleType> for TsTypeDef {
  fn from(other: &TsTupleType) -> TsTypeDef {
    let mut type_defs = vec![];

    for type_box in &other.elem_types {
      let ts_type: &TsType = &type_box.ty;
      let def: TsTypeDef = ts_type.into();
      type_defs.push(def)
    }

    TsTypeDef {
      tuple: Some(type_defs),
      kind: Some(TsTypeDefKind::Tuple),
      ..Default::default()
    }
  }
}

impl From<&TsUnionOrIntersectionType> for TsTypeDef {
  fn from(other: &TsUnionOrIntersectionType) -> TsTypeDef {
    use deno_ast::swc::ast::TsUnionOrIntersectionType::*;

    match other {
      TsUnionType(union_type) => {
        let mut types_union = vec![];

        for type_box in &union_type.types {
          let ts_type: &TsType = &(*type_box);
          let def: TsTypeDef = ts_type.into();
          types_union.push(def);
        }

        TsTypeDef {
          union: Some(types_union),
          kind: Some(TsTypeDefKind::Union),
          ..Default::default()
        }
      }
      TsIntersectionType(intersection_type) => {
        let mut types_intersection = vec![];

        for type_box in &intersection_type.types {
          let ts_type: &TsType = &(*type_box);
          let def: TsTypeDef = ts_type.into();
          types_intersection.push(def);
        }

        TsTypeDef {
          intersection: Some(types_intersection),
          kind: Some(TsTypeDefKind::Intersection),
          ..Default::default()
        }
      }
    }
  }
}

impl From<&TsKeywordType> for TsTypeDef {
  fn from(other: &TsKeywordType) -> TsTypeDef {
    use deno_ast::swc::ast::TsKeywordTypeKind::*;

    let keyword_str = match other.kind {
      TsAnyKeyword => "any",
      TsUnknownKeyword => "unknown",
      TsNumberKeyword => "number",
      TsObjectKeyword => "object",
      TsBooleanKeyword => "boolean",
      TsBigIntKeyword => "bigint",
      TsStringKeyword => "string",
      TsSymbolKeyword => "symbol",
      TsVoidKeyword => "void",
      TsUndefinedKeyword => "undefined",
      TsNullKeyword => "null",
      TsNeverKeyword => "never",
      TsIntrinsicKeyword => "intrinsic",
    };

    TsTypeDef::keyword(keyword_str)
  }
}

impl From<&TsTypeOperator> for TsTypeDef {
  fn from(other: &TsTypeOperator) -> TsTypeDef {
    let ts_type = (&*other.type_ann).into();
    let type_operator_def = TsTypeOperatorDef {
      operator: other.op.as_str().to_string(),
      ts_type,
    };

    TsTypeDef {
      type_operator: Some(Box::new(type_operator_def)),
      kind: Some(TsTypeDefKind::TypeOperator),
      ..Default::default()
    }
  }
}

impl From<&TsParenthesizedType> for TsTypeDef {
  fn from(other: &TsParenthesizedType) -> TsTypeDef {
    let ts_type = (&*other.type_ann).into();

    TsTypeDef {
      parenthesized: Some(Box::new(ts_type)),
      kind: Some(TsTypeDefKind::Parenthesized),
      ..Default::default()
    }
  }
}

impl From<&TsRestType> for TsTypeDef {
  fn from(other: &TsRestType) -> TsTypeDef {
    let ts_type = (&*other.type_ann).into();

    TsTypeDef {
      rest: Some(Box::new(ts_type)),
      kind: Some(TsTypeDefKind::Rest),
      ..Default::default()
    }
  }
}

impl From<&TsOptionalType> for TsTypeDef {
  fn from(other: &TsOptionalType) -> TsTypeDef {
    let ts_type = (&*other.type_ann).into();

    TsTypeDef {
      optional: Some(Box::new(ts_type)),
      kind: Some(TsTypeDefKind::Optional),
      ..Default::default()
    }
  }
}

impl From<&TsThisType> for TsTypeDef {
  fn from(_: &TsThisType) -> TsTypeDef {
    TsTypeDef {
      repr: "this".to_string(),
      this: Some(true),
      kind: Some(TsTypeDefKind::This),
      ..Default::default()
    }
  }
}

impl From<&TsTypePredicate> for TsTypeDef {
  fn from(other: &TsTypePredicate) -> TsTypeDef {
    let pred = TsTypePredicateDef {
      asserts: other.asserts,
      param: (&other.param_name).into(),
      r#type: other
        .type_ann
        .as_ref()
        .map(|t| Box::new(ts_type_ann_to_def(t))),
    };
    TsTypeDef {
      repr: pred.to_string(),
      kind: Some(TsTypeDefKind::TypePredicate),
      type_predicate: Some(pred),
      ..Default::default()
    }
  }
}

pub fn ts_entity_name_to_name(
  entity_name: &deno_ast::swc::ast::TsEntityName,
) -> String {
  use deno_ast::swc::ast::TsEntityName::*;

  match entity_name {
    Ident(ident) => ident.sym.to_string(),
    TsQualifiedName(ts_qualified_name) => {
      let left = ts_entity_name_to_name(&ts_qualified_name.left);
      let right = ts_member_name_to_name(&ts_qualified_name.right);
      format!("{}.{}", left, right)
    }
  }
}

pub fn ts_member_name_to_name(
  member_name: &deno_ast::swc::ast::TsMemberName,
) -> String {
  use deno_ast::swc::ast::TsMemberName::*;

  match member_name {
    Ident(ident) => ident.sym.to_string(),
    PrivateName(private_name) => private_name.id.sym.to_string(),
  }
}

impl From<&TsTypeQuery> for TsTypeDef {
  fn from(other: &TsTypeQuery) -> TsTypeDef {
    use deno_ast::swc::ast::TsTypeQueryExpr::*;

    let type_name = match &other.expr_name {
      TsEntityName(entity_name) => ts_entity_name_to_name(&*entity_name),
      Import(import_type) => import_type.arg.value.to_string(),
    };

    TsTypeDef {
      repr: type_name.to_string(),
      type_query: Some(type_name),
      kind: Some(TsTypeDefKind::TypeQuery),
      ..Default::default()
    }
  }
}

impl From<&TsTypeRef> for TsTypeDef {
  fn from(other: &TsTypeRef) -> TsTypeDef {
    let type_name = ts_entity_name_to_name(&other.type_name);

    let type_params = if let Some(type_params_inst) = &other.type_params {
      let mut ts_type_defs = vec![];

      for type_box in &type_params_inst.params {
        let ts_type: &TsType = &(*type_box);
        let def: TsTypeDef = ts_type.into();
        ts_type_defs.push(def);
      }

      Some(ts_type_defs)
    } else {
      None
    };

    TsTypeDef {
      repr: type_name.clone(),
      type_ref: Some(TsTypeRefDef {
        type_params,
        type_name,
      }),
      kind: Some(TsTypeDefKind::TypeRef),
      ..Default::default()
    }
  }
}

impl From<&TsExprWithTypeArgs> for TsTypeDef {
  fn from(other: &TsExprWithTypeArgs) -> TsTypeDef {
    let type_name = expr_to_name(&other.expr);

    let type_params = if let Some(type_params_inst) = &other.type_args {
      let mut ts_type_defs = vec![];

      for type_box in &type_params_inst.params {
        let ts_type: &TsType = &(*type_box);
        let def: TsTypeDef = ts_type.into();
        ts_type_defs.push(def);
      }

      Some(ts_type_defs)
    } else {
      None
    };

    TsTypeDef {
      repr: type_name.clone(),
      type_ref: Some(TsTypeRefDef {
        type_params,
        type_name,
      }),
      kind: Some(TsTypeDefKind::TypeRef),
      ..Default::default()
    }
  }
}

impl From<&TsIndexedAccessType> for TsTypeDef {
  fn from(other: &TsIndexedAccessType) -> TsTypeDef {
    let indexed_access_def = TsIndexedAccessDef {
      readonly: other.readonly,
      obj_type: Box::new((&*other.obj_type).into()),
      index_type: Box::new((&*other.index_type).into()),
    };

    TsTypeDef {
      indexed_access: Some(indexed_access_def),
      kind: Some(TsTypeDefKind::IndexedAccess),
      ..Default::default()
    }
  }
}

impl From<&TsMappedType> for TsTypeDef {
  fn from(other: &TsMappedType) -> Self {
    let mapped_type_def = TsMappedTypeDef {
      readonly: other.readonly,
      type_param: Box::new((&other.type_param).into()),
      name_type: other
        .name_type
        .as_ref()
        .map(|nt| Box::new(TsTypeDef::from(&**nt))),
      optional: other.optional,
      ts_type: other
        .type_ann
        .as_ref()
        .map(|a| Box::new(TsTypeDef::from(&**a))),
    };

    TsTypeDef {
      mapped_type: Some(mapped_type_def),
      kind: Some(TsTypeDefKind::Mapped),
      ..Default::default()
    }
  }
}

impl From<&TsTypeLit> for TsTypeDef {
  fn from(other: &TsTypeLit) -> TsTypeDef {
    let mut methods = vec![];
    let mut properties = vec![];
    let mut call_signatures = vec![];
    let mut index_signatures = vec![];

    for type_element in &other.members {
      use deno_ast::swc::ast::TsTypeElement::*;

      match &type_element {
        TsMethodSignature(ts_method_sig) => {
          let mut params = vec![];

          for param in &ts_method_sig.params {
            let param_def = ts_fn_param_to_param_def(None, param);
            params.push(param_def);
          }

          let maybe_return_type = ts_method_sig
            .type_ann
            .as_ref()
            .map(|rt| (&*rt.type_ann).into());

          let type_params = maybe_type_param_decl_to_type_param_defs(
            ts_method_sig.type_params.as_ref(),
          );
          let name = expr_to_name(&*ts_method_sig.key);
          let method_def = LiteralMethodDef {
            name,
            kind: deno_ast::swc::ast::MethodKind::Method,
            params,
            computed: ts_method_sig.computed,
            optional: ts_method_sig.optional,
            return_type: maybe_return_type,
            type_params,
          };
          methods.push(method_def);
        }
        TsGetterSignature(ts_getter_sig) => {
          let maybe_return_type = ts_getter_sig
            .type_ann
            .as_ref()
            .map(|rt| (&*rt.type_ann).into());

          let name = expr_to_name(&*ts_getter_sig.key);
          let method_def = LiteralMethodDef {
            name,
            kind: deno_ast::swc::ast::MethodKind::Getter,
            params: vec![],
            computed: ts_getter_sig.computed,
            optional: ts_getter_sig.optional,
            return_type: maybe_return_type,
            type_params: vec![],
          };
          methods.push(method_def);
        }
        TsSetterSignature(ts_setter_sig) => {
          let name = expr_to_name(&*ts_setter_sig.key);

          let params =
            vec![ts_fn_param_to_param_def(None, &ts_setter_sig.param)];

          let method_def = LiteralMethodDef {
            name,
            kind: deno_ast::swc::ast::MethodKind::Setter,
            params,
            computed: ts_setter_sig.computed,
            optional: ts_setter_sig.optional,
            return_type: None,
            type_params: vec![],
          };
          methods.push(method_def);
        }
        TsPropertySignature(ts_prop_sig) => {
          let name = expr_to_name(&*ts_prop_sig.key);

          let mut params = vec![];

          for param in &ts_prop_sig.params {
            let param_def = ts_fn_param_to_param_def(None, param);
            params.push(param_def);
          }

          let ts_type = ts_prop_sig
            .type_ann
            .as_ref()
            .map(|rt| (&*rt.type_ann).into());

          let type_params = maybe_type_param_decl_to_type_param_defs(
            ts_prop_sig.type_params.as_ref(),
          );
          let prop_def = LiteralPropertyDef {
            name,
            params,
            ts_type,
            readonly: ts_prop_sig.readonly,
            computed: ts_prop_sig.computed,
            optional: ts_prop_sig.optional,
            type_params,
          };
          properties.push(prop_def);
        }
        TsCallSignatureDecl(ts_call_sig) => {
          let mut params = vec![];
          for param in &ts_call_sig.params {
            let param_def = ts_fn_param_to_param_def(None, param);
            params.push(param_def);
          }

          let ts_type = ts_call_sig
            .type_ann
            .as_ref()
            .map(|rt| (&*rt.type_ann).into());

          let type_params = maybe_type_param_decl_to_type_param_defs(
            ts_call_sig.type_params.as_ref(),
          );

          let call_sig_def = LiteralCallSignatureDef {
            params,
            ts_type,
            type_params,
          };
          call_signatures.push(call_sig_def);
        }
        TsIndexSignature(ts_index_sig) => {
          let mut params = vec![];
          for param in &ts_index_sig.params {
            let param_def = ts_fn_param_to_param_def(None, param);
            params.push(param_def);
          }

          let ts_type = ts_index_sig
            .type_ann
            .as_ref()
            .map(|rt| (&*rt.type_ann).into());

          let index_sig_def = LiteralIndexSignatureDef {
            readonly: ts_index_sig.readonly,
            params,
            ts_type,
          };
          index_signatures.push(index_sig_def);
        }
        TsConstructSignatureDecl(ts_construct_sig) => {
          let mut params = vec![];
          for param in &ts_construct_sig.params {
            let param_def = ts_fn_param_to_param_def(None, param);
            params.push(param_def);
          }

          let type_params = maybe_type_param_decl_to_type_param_defs(
            ts_construct_sig.type_params.as_ref(),
          );

          let maybe_return_type = ts_construct_sig
            .type_ann
            .as_ref()
            .map(|rt| (&*rt.type_ann).into());

          let construct_sig_def = LiteralMethodDef {
            name: "new".to_string(),
            kind: deno_ast::swc::ast::MethodKind::Method,
            computed: false,
            optional: false,
            params,
            return_type: maybe_return_type,
            type_params,
          };

          methods.push(construct_sig_def);
        }
      }
    }

    let type_literal = TsTypeLiteralDef {
      methods,
      properties,
      call_signatures,
      index_signatures,
    };

    TsTypeDef {
      kind: Some(TsTypeDefKind::TypeLiteral),
      type_literal: Some(type_literal),
      ..Default::default()
    }
  }
}

impl From<&TsConditionalType> for TsTypeDef {
  fn from(other: &TsConditionalType) -> TsTypeDef {
    let conditional_type_def = TsConditionalDef {
      check_type: Box::new((&*other.check_type).into()),
      extends_type: Box::new((&*other.extends_type).into()),
      true_type: Box::new((&*other.true_type).into()),
      false_type: Box::new((&*other.false_type).into()),
    };

    TsTypeDef {
      kind: Some(TsTypeDefKind::Conditional),
      conditional_type: Some(conditional_type_def),
      ..Default::default()
    }
  }
}

impl From<&TsInferType> for TsTypeDef {
  fn from(other: &TsInferType) -> Self {
    let infer = TsInferDef {
      type_param: Box::new((&other.type_param).into()),
    };

    Self {
      kind: Some(TsTypeDefKind::Infer),
      infer: Some(infer),
      ..Default::default()
    }
  }
}

impl From<&TsImportType> for TsTypeDef {
  fn from(other: &TsImportType) -> Self {
    let type_params = if let Some(type_params_inst) = &other.type_args {
      let mut ts_type_defs = vec![];

      for type_box in &type_params_inst.params {
        let ts_type: &TsType = &(*type_box);
        let def: TsTypeDef = ts_type.into();
        ts_type_defs.push(def);
      }

      Some(ts_type_defs)
    } else {
      None
    };

    let import_type_def = TsImportTypeDef {
      specifier: other.arg.value.to_string(),
      qualifier: other.qualifier.as_ref().map(ts_entity_name_to_name),
      type_params,
    };

    Self {
      kind: Some(TsTypeDefKind::ImportType),
      import_type: Some(import_type_def),
      ..Default::default()
    }
  }
}

impl From<&TsFnOrConstructorType> for TsTypeDef {
  fn from(other: &TsFnOrConstructorType) -> TsTypeDef {
    use deno_ast::swc::ast::TsFnOrConstructorType::*;

    let fn_def = match other {
      TsFnType(ts_fn_type) => {
        let mut params = vec![];

        for param in &ts_fn_type.params {
          let param_def = ts_fn_param_to_param_def(None, param);
          params.push(param_def);
        }

        let type_params = maybe_type_param_decl_to_type_param_defs(
          ts_fn_type.type_params.as_ref(),
        );

        TsFnOrConstructorDef {
          constructor: false,
          ts_type: ts_type_ann_to_def(&ts_fn_type.type_ann),
          params,
          type_params,
        }
      }
      TsConstructorType(ctor_type) => {
        let mut params = vec![];

        for param in &ctor_type.params {
          let param_def = ts_fn_param_to_param_def(None, param);
          params.push(param_def);
        }

        let type_params = maybe_type_param_decl_to_type_param_defs(
          ctor_type.type_params.as_ref(),
        );
        TsFnOrConstructorDef {
          constructor: true,
          ts_type: ts_type_ann_to_def(&ctor_type.type_ann),
          params,
          type_params,
        }
      }
    };

    TsTypeDef {
      kind: Some(TsTypeDefKind::FnOrConstructor),
      fn_or_constructor: Some(Box::new(fn_def)),
      ..Default::default()
    }
  }
}

impl From<&TsType> for TsTypeDef {
  fn from(other: &TsType) -> TsTypeDef {
    use deno_ast::swc::ast::TsType::*;

    match other {
      TsKeywordType(keyword_type) => keyword_type.into(),
      TsThisType(this_type) => this_type.into(),
      TsFnOrConstructorType(fn_or_con_type) => fn_or_con_type.into(),
      TsTypeRef(type_ref) => type_ref.into(),
      TsTypeQuery(type_query) => type_query.into(),
      TsTypeLit(type_literal) => type_literal.into(),
      TsArrayType(array_type) => array_type.into(),
      TsTupleType(tuple_type) => tuple_type.into(),
      TsOptionalType(optional_type) => optional_type.into(),
      TsRestType(rest_type) => rest_type.into(),
      TsUnionOrIntersectionType(union_or_inter) => union_or_inter.into(),
      TsConditionalType(conditional_type) => conditional_type.into(),
      TsInferType(infer_type) => infer_type.into(),
      TsParenthesizedType(paren_type) => paren_type.into(),
      TsTypeOperator(type_op_type) => type_op_type.into(),
      TsIndexedAccessType(indexed_access_type) => indexed_access_type.into(),
      TsMappedType(mapped_type) => mapped_type.into(),
      TsLitType(lit_type) => lit_type.into(),
      TsTypePredicate(type_predicate_type) => type_predicate_type.into(),
      TsImportType(import_type) => import_type.into(),
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsTypeRefDef {
  pub type_params: Option<Vec<TsTypeDef>>,
  pub type_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LiteralDefKind {
  Number,
  String,
  Template,
  Boolean,
  BigInt,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiteralDef {
  pub kind: LiteralDefKind,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub number: Option<f64>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub string: Option<String>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub ts_types: Option<Vec<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub boolean: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsTypeOperatorDef {
  pub operator: String,
  pub ts_type: TsTypeDef,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsFnOrConstructorDef {
  pub constructor: bool,
  pub ts_type: TsTypeDef,
  pub params: Vec<ParamDef>,
  pub type_params: Vec<TsTypeParamDef>,
}

impl From<&deno_ast::swc::ast::ArrowExpr> for TsFnOrConstructorDef {
  fn from(expr: &deno_ast::swc::ast::ArrowExpr) -> Self {
    let params = expr
      .params
      .iter()
      .map(|pat| pat_to_param_def(None, pat))
      .collect();
    let ts_type = expr
      .return_type
      .as_ref()
      .map(ts_type_ann_to_def)
      .unwrap_or_else(|| TsTypeDef::keyword("unknown"));
    let type_params =
      maybe_type_param_decl_to_type_param_defs(expr.type_params.as_ref());

    Self {
      constructor: false,
      ts_type,
      params,
      type_params,
    }
  }
}

impl From<&deno_ast::swc::ast::FnExpr> for TsFnOrConstructorDef {
  fn from(expr: &deno_ast::swc::ast::FnExpr) -> Self {
    let params = expr
      .function
      .params
      .iter()
      .map(|param| pat_to_param_def(None, &param.pat))
      .collect();
    let ts_type = expr
      .function
      .return_type
      .as_ref()
      .map(ts_type_ann_to_def)
      .unwrap_or_else(|| TsTypeDef::keyword("unknown"));
    let type_params = maybe_type_param_decl_to_type_param_defs(
      expr.function.type_params.as_ref(),
    );

    Self {
      constructor: false,
      ts_type,
      params,
      type_params,
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsConditionalDef {
  pub check_type: Box<TsTypeDef>,
  pub extends_type: Box<TsTypeDef>,
  pub true_type: Box<TsTypeDef>,
  pub false_type: Box<TsTypeDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsInferDef {
  pub type_param: Box<TsTypeParamDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsImportTypeDef {
  pub specifier: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub qualifier: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub type_params: Option<Vec<TsTypeDef>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsIndexedAccessDef {
  pub readonly: bool,
  pub obj_type: Box<TsTypeDef>,
  pub index_type: Box<TsTypeDef>,
}

/// Mapped Types
///
/// ```ts
/// readonly [Properties in keyof Type as NewType]: Type[Properties]
/// ```
///
/// - `readonly` = `TruePlusMinus::True`
/// - `type_param` = `Some(TsTypeParamDef)` (`Properties in keyof Type`)
/// - `name_type` = `Some(TsTypeDef)` (`NewType`)
/// - `optional` = `None`
/// - `ts_type` = `Some(TsTypeDef)` (`Type[Properties]`)
///
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsMappedTypeDef {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub readonly: Option<TruePlusMinus>,
  pub type_param: Box<TsTypeParamDef>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name_type: Option<Box<TsTypeDef>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub optional: Option<TruePlusMinus>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ts_type: Option<Box<TsTypeDef>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiteralMethodDef {
  pub name: String,
  pub kind: deno_ast::swc::ast::MethodKind,
  pub params: Vec<ParamDef>,
  #[serde(skip_serializing_if = "is_false")]
  pub computed: bool,
  pub optional: bool,
  pub return_type: Option<TsTypeDef>,
  pub type_params: Vec<TsTypeParamDef>,
}

impl Display for LiteralMethodDef {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(
      f,
      "{}{}({})",
      display_computed(self.computed, &self.name),
      display_optional(self.optional),
      SliceDisplayer::new(&self.params, ", ", false)
    )?;
    if let Some(return_type) = &self.return_type {
      write!(f, ": {}", return_type)?;
    }
    Ok(())
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiteralPropertyDef {
  pub name: String,
  pub params: Vec<ParamDef>,
  #[serde(skip_serializing_if = "is_false")]
  pub readonly: bool,
  pub computed: bool,
  pub optional: bool,
  pub ts_type: Option<TsTypeDef>,
  pub type_params: Vec<TsTypeParamDef>,
}

impl Display for LiteralPropertyDef {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "{}", self.name)?;
    if let Some(ts_type) = &self.ts_type {
      write!(f, ": {}", ts_type)?;
    }
    Ok(())
  }
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiteralCallSignatureDef {
  pub params: Vec<ParamDef>,
  pub ts_type: Option<TsTypeDef>,
  pub type_params: Vec<TsTypeParamDef>,
}

impl Display for LiteralCallSignatureDef {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "({})", SliceDisplayer::new(&self.params, ", ", false))?;
    if let Some(ts_type) = &self.ts_type {
      write!(f, ": {}", ts_type)?;
    }
    Ok(())
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiteralIndexSignatureDef {
  pub readonly: bool,
  pub params: Vec<ParamDef>,
  pub ts_type: Option<TsTypeDef>,
}

impl Display for LiteralIndexSignatureDef {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(
      f,
      "{}[{}]",
      display_readonly(self.readonly),
      SliceDisplayer::new(&self.params, ", ", false)
    )?;
    if let Some(ts_type) = &self.ts_type {
      write!(f, ": {}", ts_type)?;
    }
    Ok(())
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsTypeLiteralDef {
  pub methods: Vec<LiteralMethodDef>,
  pub properties: Vec<LiteralPropertyDef>,
  pub call_signatures: Vec<LiteralCallSignatureDef>,
  pub index_signatures: Vec<LiteralIndexSignatureDef>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum TsTypeDefKind {
  Keyword,
  Literal,
  TypeRef,
  Union,
  Intersection,
  Array,
  Tuple,
  TypeOperator,
  Parenthesized,
  Rest,
  Optional,
  TypeQuery,
  This,
  FnOrConstructor,
  Conditional,
  Infer,
  IndexedAccess,
  Mapped,
  TypeLiteral,
  TypePredicate,
  ImportType,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsTypeDef {
  pub repr: String,

  pub kind: Option<TsTypeDefKind>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub keyword: Option<String>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub literal: Option<LiteralDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub type_ref: Option<TsTypeRefDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub union: Option<Vec<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub intersection: Option<Vec<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub array: Option<Box<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub tuple: Option<Vec<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub type_operator: Option<Box<TsTypeOperatorDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub parenthesized: Option<Box<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub rest: Option<Box<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub optional: Option<Box<TsTypeDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub type_query: Option<String>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub this: Option<bool>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub fn_or_constructor: Option<Box<TsFnOrConstructorDef>>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub conditional_type: Option<TsConditionalDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub infer: Option<TsInferDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub indexed_access: Option<TsIndexedAccessDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub mapped_type: Option<TsMappedTypeDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub type_literal: Option<TsTypeLiteralDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub type_predicate: Option<TsTypePredicateDef>,

  #[serde(skip_serializing_if = "Option::is_none")]
  pub import_type: Option<TsImportTypeDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ThisOrIdent {
  This,
  Identifier { name: String },
}

impl From<&TsThisTypeOrIdent> for ThisOrIdent {
  fn from(other: &TsThisTypeOrIdent) -> ThisOrIdent {
    use TsThisTypeOrIdent::*;
    match other {
      TsThisType(_) => Self::This,
      Ident(ident) => Self::Identifier {
        name: ident.sym.to_string(),
      },
    }
  }
}

/// ```ts
/// function foo(param: any): asserts param is SomeType { ... }
///                           ^^^^^^^ ^^^^^    ^^^^^^^^
///                           (1)     (2)      (3)
/// ```
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TsTypePredicateDef {
  /// (1) Whether the predicate includes `asserts` keyword or not
  pub asserts: bool,

  /// (2) The term of predicate
  pub param: ThisOrIdent,

  /// (3) The type against which the parameter is checked
  pub r#type: Option<Box<TsTypeDef>>,
}

impl Display for TsTypePredicateDef {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    let mut s = Vec::new();
    if self.asserts {
      s.push("asserts".to_string());
    }
    s.push(match &self.param {
      ThisOrIdent::This => "this".to_string(),
      ThisOrIdent::Identifier { name } => name.clone(),
    });
    if let Some(ty) = &self.r#type {
      s.push("is".to_string());
      s.push(ty.to_string());
    }
    write!(f, "{}", s.join(" "))
  }
}

fn get_span_from_type(ts_type: &TsType) -> Span {
  use deno_ast::swc::ast::TsType::*;

  match ts_type {
    TsArrayType(ref t) => get_span_from_type(t.elem_type.as_ref()),
    TsConditionalType(ref t) => get_span_from_type(t.check_type.as_ref()),
    TsFnOrConstructorType(ref t) => {
      if let Some(t) = t.clone().ts_constructor_type() {
        t.span
      } else if let Some(t) = t.clone().ts_fn_type() {
        t.span
      } else {
        unreachable!("no type found")
      }
    }
    TsImportType(ref t) => t.span,
    TsIndexedAccessType(ref t) => get_span_from_type(&t.index_type),
    TsInferType(t) => t.span,
    TsKeywordType(t) => t.span,
    TsLitType(t) => t.span,
    TsMappedType(t) => t.span,
    TsOptionalType(t) => t.span,
    TsParenthesizedType(t) => t.span,
    TsRestType(t) => t.span,
    TsThisType(t) => t.span,
    TsTupleType(t) => t.span,
    TsTypeLit(t) => t.span,
    TsTypeOperator(t) => t.span,
    TsTypePredicate(t) => t.span,
    TsTypeQuery(t) => t.span,
    TsTypeRef(t) => t.span,
    TsUnionOrIntersectionType(t) => {
      if let Some(t) = t.clone().ts_intersection_type() {
        t.span
      } else if let Some(t) = t.clone().ts_union_type() {
        t.span
      } else {
        unreachable!("no type found")
      }
    }
  }
}

impl TsTypeDef {
  pub fn number_literal(num: &Number) -> Self {
    let repr = format!("{}", num.value);
    let lit = LiteralDef {
      kind: LiteralDefKind::Number,
      number: Some(num.value),
      string: None,
      ts_types: None,
      boolean: None,
    };
    Self::literal(repr, lit)
  }

  pub fn string_literal(str_: &Str) -> Self {
    let repr = str_.value.to_string();
    let lit = LiteralDef {
      kind: LiteralDefKind::String,
      number: None,
      string: Some(str_.value.to_string()),
      ts_types: None,
      boolean: None,
    };
    Self::literal(repr, lit)
  }

  pub fn tpl_literal(types: &[Box<TsType>], quasis: &[TplElement]) -> Self {
    let mut ts_types: Vec<(Span, Self, String)> = Vec::new();
    for ts_type in types {
      let t: Self = ts_type.as_ref().into();
      let repr = format!("${{{}}}", t);
      ts_types.push((get_span_from_type(ts_type), t, repr))
    }
    for quasi in quasis {
      let repr = quasi.raw.to_string();
      let lit = LiteralDef {
        kind: LiteralDefKind::String,
        number: None,
        string: Some(repr.clone()),
        ts_types: None,
        boolean: None,
      };
      ts_types.push((quasi.span, Self::literal(repr.clone(), lit), repr));
    }
    ts_types.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
    let repr = ts_types
      .iter()
      .map(|(_, _, s)| s.as_str())
      .collect::<Vec<&str>>()
      .join("");
    let ts_types = Some(ts_types.into_iter().map(|(_, t, _)| t).collect());
    let lit = LiteralDef {
      kind: LiteralDefKind::Template,
      number: None,
      string: None,
      ts_types,
      boolean: None,
    };
    Self::literal(repr, lit)
  }

  pub fn bool_literal(bool_: &Bool) -> Self {
    let repr = bool_.value.to_string();
    let lit = LiteralDef {
      kind: LiteralDefKind::Boolean,
      number: None,
      string: None,
      ts_types: None,
      boolean: Some(bool_.value),
    };
    Self::literal(repr, lit)
  }

  pub fn bigint_literal(bigint_: &BigInt) -> Self {
    let repr = bigint_.value.to_string();
    let lit = LiteralDef {
      kind: LiteralDefKind::BigInt,
      number: None,
      string: Some(bigint_.value.to_string()),
      ts_types: None,
      boolean: None,
    };
    Self::literal(repr, lit)
  }

  pub fn regexp(repr: String) -> Self {
    Self {
      repr,
      kind: Some(TsTypeDefKind::TypeRef),
      type_ref: Some(TsTypeRefDef {
        type_params: None,
        type_name: "RegExp".to_string(),
      }),
      ..Default::default()
    }
  }

  pub fn keyword(keyword_str: &str) -> Self {
    Self::keyword_with_repr(keyword_str, keyword_str)
  }

  pub fn number_with_repr(repr: &str) -> Self {
    Self::keyword_with_repr("number", repr)
  }

  pub fn string_with_repr(repr: &str) -> Self {
    Self::keyword_with_repr("string", repr)
  }

  pub fn bool_with_repr(repr: &str) -> Self {
    Self::keyword_with_repr("boolean", repr)
  }

  pub fn bigint_with_repr(repr: &str) -> Self {
    Self::keyword_with_repr("bigint", repr)
  }

  pub fn keyword_with_repr(keyword_str: &str, repr: &str) -> Self {
    Self {
      repr: repr.to_string(),
      kind: Some(TsTypeDefKind::Keyword),
      keyword: Some(keyword_str.to_string()),
      ..Default::default()
    }
  }

  fn literal(repr: String, lit: LiteralDef) -> Self {
    Self {
      repr,
      kind: Some(TsTypeDefKind::Literal),
      literal: Some(lit),
      ..Default::default()
    }
  }
}

pub fn ts_type_ann_to_def(type_ann: &TsTypeAnn) -> TsTypeDef {
  use deno_ast::swc::ast::TsType::*;

  match &*type_ann.type_ann {
    TsKeywordType(keyword_type) => keyword_type.into(),
    TsThisType(this_type) => this_type.into(),
    TsFnOrConstructorType(fn_or_con_type) => fn_or_con_type.into(),
    TsTypeRef(type_ref) => type_ref.into(),
    TsTypeQuery(type_query) => type_query.into(),
    TsTypeLit(type_literal) => type_literal.into(),
    TsArrayType(array_type) => array_type.into(),
    TsTupleType(tuple_type) => tuple_type.into(),
    TsOptionalType(optional_type) => optional_type.into(),
    TsRestType(rest_type) => rest_type.into(),
    TsUnionOrIntersectionType(union_or_inter) => union_or_inter.into(),
    TsConditionalType(conditional_type) => conditional_type.into(),
    TsInferType(infer_type) => infer_type.into(),
    TsParenthesizedType(paren_type) => paren_type.into(),
    TsTypeOperator(type_op_type) => type_op_type.into(),
    TsIndexedAccessType(indexed_access_type) => indexed_access_type.into(),
    TsMappedType(mapped_type) => mapped_type.into(),
    TsLitType(lit_type) => lit_type.into(),
    TsTypePredicate(type_predicate) => type_predicate.into(),
    TsImportType(import_type) => import_type.into(),
  }
}

pub fn infer_ts_type_from_expr(
  expr: &Expr,
  is_const: bool,
) -> Option<TsTypeDef> {
  match expr {
    Expr::Array(arr_lit) => {
      // e.g.) const n = ["a", 1];
      infer_ts_type_from_arr_lit(arr_lit, false)
    }
    Expr::Arrow(expr) => {
      // e.g.) const f = (a: string): void => {};
      infer_ts_type_from_arrow_expr(expr)
    }
    Expr::Fn(expr) => {
      // e.g.) const f = function a(a:string): void {};
      infer_ts_type_from_fn_expr(expr)
    }
    Expr::Lit(lit) => {
      // e.g.) const n = 100;
      infer_ts_type_from_lit(lit, is_const)
    }
    Expr::New(expr) => {
      // e.g.) const d = new Date()
      infer_ts_type_from_new_expr(expr)
    }
    Expr::Tpl(tpl) => {
      // e.g.) const s = `hello`;
      Some(infer_ts_type_from_tpl(tpl, is_const))
    }
    Expr::TsConstAssertion(assertion) => {
      // e.g.) const s = [] as const;
      infer_ts_type_from_const_assertion(assertion)
    }
    Expr::Call(expr) => {
      // e.g.) const value = Number(123);
      infer_ts_type_from_call_expr(expr)
    }
    _ => None,
  }
}

pub fn infer_simple_ts_type_from_var_decl(
  decl: &VarDeclarator,
  is_const: bool,
) -> Option<TsTypeDef> {
  if let Some(init_expr) = &decl.init {
    infer_ts_type_from_expr(init_expr.as_ref(), is_const)
  } else {
    None
  }
}

fn infer_ts_type_from_arr_lit(
  arr_lit: &ArrayLit,
  is_const: bool,
) -> Option<TsTypeDef> {
  let mut defs = Vec::new();
  for expr in arr_lit.elems.iter().flatten() {
    if expr.spread.is_none() {
      if let Some(ts_type) = infer_ts_type_from_expr(&expr.expr, is_const) {
        if !defs.contains(&ts_type) {
          defs.push(ts_type);
        }
      } else {
        // it is not a trivial type that can be inferred an so will infer an
        // an any array.
        return Some(TsTypeDef {
          repr: "any[]".to_string(),
          kind: Some(TsTypeDefKind::Array),
          array: Some(Box::new(TsTypeDef::keyword("any"))),
          ..Default::default()
        });
      }
    } else {
      // TODO(@kitsonk) we should recursively unwrap the spread here
      return Some(TsTypeDef {
        repr: "any[]".to_string(),
        kind: Some(TsTypeDefKind::Array),
        array: Some(Box::new(TsTypeDef::keyword("any"))),
        ..Default::default()
      });
    }
  }
  match defs.len() {
    1 => Some(TsTypeDef {
      kind: Some(TsTypeDefKind::Array),
      array: Some(Box::new(defs[0].clone())),
      ..Default::default()
    }),
    2.. => {
      let union = TsTypeDef {
        kind: Some(TsTypeDefKind::Union),
        union: Some(defs),
        ..Default::default()
      };
      Some(TsTypeDef {
        kind: Some(TsTypeDefKind::Array),
        array: Some(Box::new(union)),
        ..Default::default()
      })
    }
    _ => None,
  }
}

fn infer_ts_type_from_arrow_expr(expr: &ArrowExpr) -> Option<TsTypeDef> {
  Some(TsTypeDef {
    kind: Some(TsTypeDefKind::FnOrConstructor),
    fn_or_constructor: Some(Box::new(expr.into())),
    ..Default::default()
  })
}

fn infer_ts_type_from_fn_expr(expr: &FnExpr) -> Option<TsTypeDef> {
  Some(TsTypeDef {
    kind: Some(TsTypeDefKind::FnOrConstructor),
    fn_or_constructor: Some(Box::new(expr.into())),
    ..Default::default()
  })
}

fn infer_ts_type_from_const_assertion(
  assertion: &TsConstAssertion,
) -> Option<TsTypeDef> {
  match &*assertion.expr {
    Expr::Array(arr_lit) => {
      // e.g.) const n = ["a", 1] as const;
      infer_ts_type_from_arr_lit(arr_lit, true)
    }
    _ => infer_ts_type_from_expr(&*assertion.expr, true),
  }
}

fn infer_ts_type_from_lit(lit: &Lit, is_const: bool) -> Option<TsTypeDef> {
  match lit {
    Lit::Num(num) => {
      if is_const {
        Some(TsTypeDef::number_literal(num))
      } else {
        Some(TsTypeDef::number_with_repr("number"))
      }
    }
    Lit::Str(str_) => {
      if is_const {
        Some(TsTypeDef::string_literal(str_))
      } else {
        Some(TsTypeDef::string_with_repr("string"))
      }
    }
    Lit::Bool(bool_) => {
      if is_const {
        Some(TsTypeDef::bool_literal(bool_))
      } else {
        Some(TsTypeDef::bool_with_repr("boolean"))
      }
    }
    Lit::BigInt(bigint_) => {
      if is_const {
        Some(TsTypeDef::bigint_literal(bigint_))
      } else {
        Some(TsTypeDef::bigint_with_repr("bigint"))
      }
    }
    Lit::Regex(regex) => Some(TsTypeDef::regexp(regex.exp.to_string())),
    _ => None,
  }
}

fn infer_ts_type_from_new_expr(new_expr: &NewExpr) -> Option<TsTypeDef> {
  match new_expr.callee.as_ref() {
    Expr::Ident(ident) => Some(TsTypeDef {
      repr: ident.sym.to_string(),
      kind: Some(TsTypeDefKind::TypeRef),
      type_ref: Some(TsTypeRefDef {
        type_params: new_expr
          .type_args
          .as_ref()
          .map(|init| maybe_type_param_instantiation_to_type_defs(Some(init))),
        type_name: ident.sym.to_string(),
      }),
      ..Default::default()
    }),
    _ => None,
  }
}

fn infer_ts_type_from_call_expr(call_expr: &CallExpr) -> Option<TsTypeDef> {
  match &call_expr.callee {
    Callee::Expr(expr) => {
      if let Expr::Ident(ident) = expr.as_ref() {
        let sym = ident.sym.to_string();
        match sym.as_str() {
          "Symbol" | "Number" | "String" | "BigInt" => {
            Some(TsTypeDef::keyword_with_repr(
              &sym.to_ascii_lowercase(),
              &sym.clone(),
            ))
          }
          "Date" => Some(TsTypeDef::string_with_repr(&sym)),
          "RegExp" => Some(TsTypeDef::regexp(sym)),
          _ => None,
        }
      } else {
        None
      }
    }
    _ => None,
  }
}

fn infer_ts_type_from_tpl(tpl: &Tpl, is_const: bool) -> TsTypeDef {
  // TODO(@kitsonk) we should iterate over the expr and if each expr has a
  // ts_type or can be trivially inferred, it should be passed to the
  // tp_literal
  if tpl.quasis.len() == 1 && is_const {
    TsTypeDef::tpl_literal(&[], &tpl.quasis)
  } else {
    TsTypeDef::string_with_repr("string")
  }
}

impl Display for TsTypeDef {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    if self.kind.is_none() {
      return write!(f, "{}", colors::red("[UNSUPPORTED]"));
    }

    let kind = self.kind.as_ref().unwrap();
    match kind {
      TsTypeDefKind::Array => {
        let array = self.array.as_ref().unwrap();
        if matches!(
          array.kind,
          Some(TsTypeDefKind::Union) | Some(TsTypeDefKind::Intersection)
        ) {
          write!(f, "({})[]", &*self.array.as_ref().unwrap())
        } else {
          write!(f, "{}[]", &*self.array.as_ref().unwrap())
        }
      }
      TsTypeDefKind::Conditional => {
        let conditional = self.conditional_type.as_ref().unwrap();
        write!(
          f,
          "{} {} {} ? {} : {}",
          &*conditional.check_type,
          colors::magenta("extends"),
          &*conditional.extends_type,
          &*conditional.true_type,
          &*conditional.false_type
        )
      }
      TsTypeDefKind::Infer => {
        let infer = self.infer.as_ref().unwrap();
        write!(f, "{} {}", colors::magenta("infer"), infer.type_param)
      }
      TsTypeDefKind::ImportType => {
        let import_type = self.import_type.as_ref().unwrap();
        write!(f, "import(\"{}\")", import_type.specifier)?;
        if let Some(qualifier) = &import_type.qualifier {
          write!(f, ".{}", qualifier)?;
        }
        if let Some(type_params) = &import_type.type_params {
          write!(f, "<{}>", SliceDisplayer::new(type_params, ", ", false))?;
        }
        Ok(())
      }
      TsTypeDefKind::FnOrConstructor => {
        let fn_or_constructor = self.fn_or_constructor.as_ref().unwrap();
        write!(
          f,
          "{}({}) => {}",
          colors::magenta(if fn_or_constructor.constructor {
            "new "
          } else {
            ""
          }),
          SliceDisplayer::new(&fn_or_constructor.params, ", ", false),
          &fn_or_constructor.ts_type,
        )
      }
      TsTypeDefKind::IndexedAccess => {
        let indexed_access = self.indexed_access.as_ref().unwrap();
        write!(
          f,
          "{}[{}]",
          &*indexed_access.obj_type, &*indexed_access.index_type
        )
      }
      TsTypeDefKind::Intersection => {
        let intersection = self.intersection.as_ref().unwrap();
        write!(f, "{}", SliceDisplayer::new(intersection, " & ", false))
      }
      TsTypeDefKind::Mapped => {
        let mapped_type = self.mapped_type.as_ref().unwrap();
        let readonly = match mapped_type.readonly {
          Some(TruePlusMinus::True) => {
            format!("{} ", colors::magenta("readonly"))
          }
          Some(TruePlusMinus::Plus) => {
            format!("+{} ", colors::magenta("readonly"))
          }
          Some(TruePlusMinus::Minus) => {
            format!("-{} ", colors::magenta("readonly"))
          }
          _ => "".to_string(),
        };
        let optional = match mapped_type.optional {
          Some(TruePlusMinus::True) => "?",
          Some(TruePlusMinus::Plus) => "+?",
          Some(TruePlusMinus::Minus) => "-?",
          _ => "",
        };
        let type_param =
          if let Some(ts_type_def) = &mapped_type.type_param.constraint {
            format!("{} in {}", mapped_type.type_param.name, ts_type_def)
          } else {
            mapped_type.type_param.to_string()
          };
        let name_type = if let Some(name_type) = &mapped_type.name_type {
          format!(" {} {}", colors::magenta("as"), name_type)
        } else {
          "".to_string()
        };
        let ts_type = if let Some(ts_type) = &mapped_type.ts_type {
          format!(": {}", ts_type)
        } else {
          "".to_string()
        };
        write!(
          f,
          "{}[{}{}]{}{}",
          readonly, type_param, name_type, optional, ts_type
        )
      }
      TsTypeDefKind::Keyword => {
        write!(f, "{}", colors::cyan(self.keyword.as_ref().unwrap()))
      }
      TsTypeDefKind::Literal => {
        let literal = self.literal.as_ref().unwrap();
        match literal.kind {
          LiteralDefKind::Boolean => write!(
            f,
            "{}",
            colors::yellow(&literal.boolean.unwrap().to_string())
          ),
          LiteralDefKind::String => write!(
            f,
            "{}",
            colors::green(&format!("\"{}\"", literal.string.as_ref().unwrap()))
          ),
          LiteralDefKind::Template => {
            write!(f, "{}", colors::green("`"))?;
            for ts_type in literal.ts_types.as_ref().unwrap() {
              let kind = ts_type.kind.as_ref().unwrap();
              if *kind == TsTypeDefKind::Literal {
                let literal = ts_type.literal.as_ref().unwrap();
                if literal.kind == LiteralDefKind::String {
                  write!(
                    f,
                    "{}",
                    colors::green(literal.string.as_ref().unwrap())
                  )?;
                  continue;
                }
              }
              write!(
                f,
                "{}{}{}",
                colors::magenta("${"),
                ts_type,
                colors::magenta("}")
              )?;
            }
            write!(f, "{}", colors::green("`"))
          }
          LiteralDefKind::Number => write!(
            f,
            "{}",
            colors::yellow(&literal.number.unwrap().to_string())
          ),
          LiteralDefKind::BigInt => {
            write!(f, "{}", colors::yellow(&literal.string.as_ref().unwrap()))
          }
        }
      }
      TsTypeDefKind::Optional => {
        write!(f, "{}?", &*self.optional.as_ref().unwrap())
      }
      TsTypeDefKind::Parenthesized => {
        write!(f, "({})", &*self.parenthesized.as_ref().unwrap())
      }
      TsTypeDefKind::Rest => write!(f, "...{}", &*self.rest.as_ref().unwrap()),
      TsTypeDefKind::This => write!(f, "this"),
      TsTypeDefKind::Tuple => {
        let tuple = self.tuple.as_ref().unwrap();
        write!(f, "[{}]", SliceDisplayer::new(tuple, ", ", false))
      }
      TsTypeDefKind::TypeLiteral => {
        let type_literal = self.type_literal.as_ref().unwrap();
        write!(
          f,
          "{{ {}{}{}{}}}",
          SliceDisplayer::new(&type_literal.call_signatures, "; ", true),
          SliceDisplayer::new(&type_literal.methods, "; ", true),
          SliceDisplayer::new(&type_literal.properties, "; ", true),
          SliceDisplayer::new(&type_literal.index_signatures, "; ", true),
        )
      }
      TsTypeDefKind::TypeOperator => {
        let operator = self.type_operator.as_ref().unwrap();
        write!(f, "{} {}", operator.operator, &operator.ts_type)
      }
      TsTypeDefKind::TypeQuery => {
        write!(f, "typeof {}", self.type_query.as_ref().unwrap())
      }
      TsTypeDefKind::TypeRef => {
        let type_ref = self.type_ref.as_ref().unwrap();
        write!(f, "{}", colors::intense_blue(&type_ref.type_name))?;
        if let Some(type_params) = &type_ref.type_params {
          write!(f, "<{}>", SliceDisplayer::new(type_params, ", ", false))?;
        }
        Ok(())
      }
      TsTypeDefKind::Union => {
        let union = self.union.as_ref().unwrap();
        write!(f, "{}", SliceDisplayer::new(union, " | ", false))
      }
      TsTypeDefKind::TypePredicate => {
        let pred = self.type_predicate.as_ref().unwrap();
        write!(f, "{}", pred)
      }
    }
  }
}

pub fn maybe_type_param_instantiation_to_type_defs(
  maybe_type_param_instantiation: Option<&TsTypeParamInstantiation>,
) -> Vec<TsTypeDef> {
  if let Some(type_param_instantiation) = maybe_type_param_instantiation {
    type_param_instantiation
      .params
      .iter()
      .map(|type_param| type_param.as_ref().into())
      .collect::<Vec<TsTypeDef>>()
  } else {
    vec![]
  }
}
