use std::io::Write;

use pgrs_core::{ExplainNode, ExplainPlan, QueryApi};

/// `\explain` / `\explain+`: ask the core for a parsed plan and render it as an
/// indented tree. All EXPLAIN/JSON knowledge lives in the core; this is pure
/// presentation.
pub(super) fn handle_explain(
    db: &QueryApi,
    sql: &str,
    analyze: bool,
    writer: &mut impl Write,
) {
    match db.explain(sql, analyze) {
        Ok(plan) => {
            write!(writer, "{}", render_plan(&plan)).ok();
        }
        Err(e) => {
            writeln!(writer, "error: {}", e).ok();
        }
    }
}

/// Render a plan tree to a string: one line per node, two-space indent per
/// depth, `->` arrows for child nodes (psql-familiar), with detail attributes
/// printed beneath each node.
fn render_plan(plan: &ExplainPlan) -> String {
    let mut out = String::new();
    render_node(&plan.root, 0, &mut out);
    out
}

fn render_node(node: &ExplainNode, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let arrow = if depth == 0 { "" } else { "-> " };
    let relation = node
        .relation
        .as_ref()
        .map(|r| format!(" on {r}"))
        .unwrap_or_default();

    let mut line = format!(
        "{indent}{arrow}{}{relation}  (cost={:.2} rows={})",
        node.node_type, node.total_cost, node.plan_rows
    );
    if let Some(t) = node.actual_time_ms {
        line.push_str(&format!(" (actual={:.3}ms rows={})", t, node.actual_rows.unwrap_or(0)));
    }
    out.push_str(&line);
    out.push('\n');

    for d in &node.detail {
        out.push_str(&format!("{indent}  {d}\n"));
    }
    for child in &node.children {
        render_node(child, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(node_type: &str) -> ExplainNode {
        ExplainNode {
            node_type: node_type.to_string(),
            relation: None,
            total_cost: 1.0,
            plan_rows: 1,
            actual_time_ms: None,
            actual_rows: None,
            detail: vec![],
            children: vec![],
        }
    }

    #[test]
    fn renders_single_node_with_cost() {
        let plan = ExplainPlan { root: ExplainNode { relation: Some("users".into()), total_cost: 18.5, plan_rows: 850, ..leaf("Seq Scan") } };
        let out = render_plan(&plan);
        assert!(out.contains("Seq Scan on users"), "got:\n{out}");
        assert!(out.contains("(cost=18.50 rows=850)"), "got:\n{out}");
        assert!(!out.contains("actual="), "no ANALYZE -> no actuals, got:\n{out}");
    }

    #[test]
    fn renders_actuals_when_present() {
        let plan = ExplainPlan { root: ExplainNode { actual_time_ms: Some(0.012), actual_rows: Some(842), ..leaf("Seq Scan") } };
        let out = render_plan(&plan);
        assert!(out.contains("actual=0.012ms rows=842"), "got:\n{out}");
    }

    #[test]
    fn renders_detail_lines() {
        let plan = ExplainPlan { root: ExplainNode { detail: vec!["Filter: (active = true)".into()], ..leaf("Seq Scan") } };
        let out = render_plan(&plan);
        assert!(out.contains("Filter: (active = true)"), "got:\n{out}");
    }

    #[test]
    fn renders_children_indented_with_arrow() {
        let plan = ExplainPlan { root: ExplainNode { children: vec![leaf("Index Scan")], ..leaf("Hash Join") } };
        let out = render_plan(&plan);
        assert!(out.contains("Hash Join"), "got:\n{out}");
        assert!(out.contains("  -> Index Scan"), "child should be indented with arrow, got:\n{out}");
    }
}
