use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Mutex},
};

use handlebars::{
    handlebars_helper, BlockParams, Context, Handlebars, Helper, HelperDef, HelperResult, Output,
    PathAndJson, RenderContext, RenderErrorReason, Renderable, Template,
};
use itertools::Itertools;
use serde::Serialize;
use serde_json::Value;

use crate::{
    generator::SiteMapper,
    page::{Breadcrumb, Breadcrumbs},
};

use handlebars::BlockContext;

pub(crate) fn create_block<'rc>(param: &PathAndJson<'rc>) -> BlockContext<'rc> {
    let mut block = BlockContext::new();

    if let Some(new_path) = param.context_path() {
        block.base_path_mut().clone_from(new_path)
    } else {
        // use clone for now
        block.set_base_value(param.value().clone());
    }

    block
}

#[derive(Clone)]
pub struct AssetHelper {
    pub mapper: HashMap<u64, String>,
}

impl HelperDef for AssetHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper,
        _hb: &Handlebars,
        context: &Context,
        _rc: &mut RenderContext,
        out: &mut dyn Output,
    ) -> HelperResult {
        let asset = h.param(0).and_then(|v| v.value().as_str()).ok_or(
            RenderErrorReason::ParamTypeMismatchForName(
                "asset",
                "0".to_string(),
                "&str".to_string(),
            ),
        )?;

        let page_id = context
            .data()
            .as_object()
            .and_then(|o| o.get("page_id"))
            .and_then(|v| v.as_u64())
            .unwrap();

        out.write(&SiteMapper::asset_url_with_mapping(
            &self.mapper,
            page_id,
            asset,
        ))?;

        // add random number afterwards to break caching
        let rand = rand::random::<u64>();
        out.write("?")?;
        out.write(&rand.to_string())?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct PaginationHelper;

impl HelperDef for PaginationHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        hb: &'reg Handlebars<'reg>,
        context: &'rc Context,
        rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let param = h
            .param(0)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("pagination", 0))?;

        let pagination =
            param
                .value()
                .as_object()
                .ok_or(RenderErrorReason::ParamTypeMismatchForName(
                    "pagination",
                    "0".into(),
                    "object".into(),
                ))?;

        let current_page = pagination
            .get("current_page")
            .and_then(|v| v.as_u64())
            .ok_or(RenderErrorReason::MissingVariable(Some(
                "current_page".into(),
            )))? as usize;
        let total_pages = pagination
            .get("total_pages")
            .and_then(|v| v.as_u64())
            .ok_or(RenderErrorReason::MissingVariable(Some(
                "total_pages".into(),
            )))? as usize;

        if total_pages == 1 {
            // nothing to do
            return Ok(());
        }

        // num before and after
        let num = h
            .param(1)
            .ok_or(RenderErrorReason::ParamNotFoundForName(
                "pagination",
                1.to_string(),
            ))
            .map(|p| p.value().as_u64())?
            .ok_or(RenderErrorReason::ParamTypeMismatchForName(
                "pagination",
                "1".into(),
                "string".into(),
            ))? as usize;

        let pages_before = (1..current_page)
            .skip(1)
            .rev()
            .take(num)
            .rev()
            .collect_vec();
        let pages_after = ((current_page + 1)..total_pages).take(num).collect_vec();

        let mut block = create_block(param);

        let mut params = BlockParams::new();

        if current_page != 1 {
            params.add_value("first_page", serde_json::to_value(1).unwrap())?;
        }
        params.add_value("pages_before", serde_json::to_value(pages_before).unwrap())?;
        params.add_value("pages_after", serde_json::to_value(pages_after).unwrap())?;
        params.add_value("current_page", serde_json::to_value(current_page).unwrap())?;
        if current_page != total_pages {
            params.add_value("last_page", serde_json::to_value(total_pages).unwrap())?;
        }

        block.set_block_params(params);

        rc.push_block(block);

        if let Some(t) = h.template() {
            t.render(hb, context, rc, out)?;
        };

        rc.pop_block();

        Ok(())
    }
}

#[derive(Clone)]
pub struct PageUrlHelper {
    pub page_to_groups: HashMap<u64, u64>,
    pub groups_to_pages: HashMap<u64, Vec<(usize, u64)>>,
    pub mapping: HashMap<u64, String>,
}

impl HelperDef for PageUrlHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper,
        _hb: &Handlebars,
        context: &Context,
        _rc: &mut RenderContext,
        out: &mut dyn Output,
    ) -> HelperResult {
        let target_page_num = h.param(0).and_then(|v| v.value().as_u64()).ok_or(
            RenderErrorReason::ParamTypeMismatchForName("page_url", "0".into(), "u64".into()),
        )? as usize;

        let current_page_id = context
            .data()
            .as_object()
            .and_then(|o| o.get("page_id"))
            .and_then(|v| v.as_u64())
            .ok_or(RenderErrorReason::MissingVariable(Some("page_id".into())))?;

        let group_id = self.page_to_groups.get(&current_page_id).unwrap();
        let target_page_id = self
            .groups_to_pages
            .get(group_id)
            .and_then(|g: &Vec<(usize, u64)>| g.iter().find(|(num, _)| *num == target_page_num))
            .map(|(_, id)| id)
            .ok_or(RenderErrorReason::Other(format!(
                "Page {} not found",
                target_page_num
            )))?;
        let target_page_path = self.mapping.get(target_page_id).unwrap();

        out.write(&SiteMapper::url_with_mapping(
            &self.mapping,
            current_page_id,
            target_page_path,
        ))?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct ColumnsHelper;

impl HelperDef for ColumnsHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        hb: &'reg Handlebars<'reg>,
        context: &'rc Context,
        rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let arr = h.param(0).and_then(|p| p.value().as_array()).ok_or(
            RenderErrorReason::ParamTypeMismatchForName("columns", "0".into(), "array".into()),
        )?;

        // nothing to do
        if arr.is_empty() {
            return Ok(());
        }

        let param = h
            .param(1)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("columns", 1))?;

        let cols = param
            .value()
            .as_u64()
            .ok_or(RenderErrorReason::ParamTypeMismatchForName(
                "columns",
                "1".into(),
                "u64".into(),
            ))?;

        let num_rows = f32::ceil((arr.len() as f32) / (cols as f32)) as usize;

        for chunk in arr.chunks(num_rows) {
            let mut block = create_block(param);

            let mut params = BlockParams::new();

            params.add_value("values", serde_json::to_value(chunk).unwrap())?;
            params.add_value("n", serde_json::to_value(cols).unwrap())?;

            block.set_block_params(params);

            rc.push_block(block);

            if let Some(t) = h.template() {
                t.render(hb, context, rc, out)?;
            };

            rc.pop_block();
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct BreadcrumbsHelper {
    pub mapping: HashMap<u64, String>,
}

impl HelperDef for BreadcrumbsHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        hb: &'reg Handlebars<'reg>,
        context: &'rc Context,
        rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let param = h
            .param(0)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("breadcrumbs", 0))?;

        let crumbs =
            serde_json::from_value::<Breadcrumbs>(param.value().clone()).map_err(|_| {
                RenderErrorReason::ParamTypeMismatchForName(
                    "breadcrumbs",
                    "0".into(),
                    "Breadcrumbs".into(),
                )
            })?;

        let page_id = context
            .data()
            .as_object()
            .and_then(|o| o.get("page_id"))
            .and_then(|v| v.as_u64())
            .ok_or(RenderErrorReason::MissingVariable(Some("page_id".into())))?;

        let len = crumbs.len();
        for (i, crumb) in crumbs.into_iter() {
            let mut block = create_block(param);
            let mut params = BlockParams::new();

            params.add_value("is_first", Value::Bool(matches!(i, _ if i == 0)))?;
            params.add_value("is_last", Value::Bool(matches!(i, _ if i == len - 1)))?;

            // add params from crumb
            match crumb {
                Breadcrumb::Single {
                    title,
                    absolute_url,
                } => {
                    params.add_value("is_paged", Value::Bool(false))?;
                    params.add_value("title", Value::String(title.clone()))?;
                    params.add_value(
                        "url",
                        Value::String(SiteMapper::url_with_mapping(
                            &self.mapping,
                            page_id,
                            &absolute_url,
                        )),
                    )
                }
                Breadcrumb::Paged {
                    title,
                    root_url,
                    page,
                } => {
                    params.add_value("is_paged", Value::Bool(true))?;
                    params.add_value("title", Value::String(title.clone()))?;
                    params.add_value(
                        "current_page",
                        serde_json::to_value(page.current_page).unwrap(),
                    )?;
                    params.add_value(
                        "total_pages",
                        serde_json::to_value(page.total_pages).unwrap(),
                    )?;
                    params.add_value(
                        "url",
                        Value::String(SiteMapper::url_with_mapping(
                            &self.mapping,
                            page_id,
                            &root_url,
                        )),
                    )
                }
            }?;

            block.set_block_params(params);
            rc.push_block(block);

            if let Some(t) = h.template() {
                t.render(hb, context, rc, out)?;
            };

            rc.pop_block();
        }

        Ok(())
    }
}
