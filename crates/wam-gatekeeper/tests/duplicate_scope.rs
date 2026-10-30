use wam_gatekeeper::{
    scenario,
    tools::{Policy, ToolCtx, Tools},
};

#[test]
fn identical_actions_on_another_route_are_not_exact_duplicate_support() {
    let mut scenario = scenario::builtin("duplicate").unwrap();
    let tasks = scenario.tasks().unwrap();
    let mut store = scenario.build_store().unwrap();
    scenario.job.candidate.action_sequence = scenario.seed[0]
        .payload
        .l2
        .steps
        .iter()
        .map(|step| step.action.clone())
        .collect();
    let tools = Tools::load().unwrap();
    let policy = Policy::default();
    assert!(
        ToolCtx::new(
            &scenario.job,
            &mut store,
            &tools,
            &policy,
            &tasks,
            scenario.now
        )
        .exact_duplicate()
        .unwrap()
        .is_some()
    );
    scenario.job.candidate.url_pattern = "/unrelated-editor".into();
    assert!(
        ToolCtx::new(
            &scenario.job,
            &mut store,
            &tools,
            &policy,
            &tasks,
            scenario.now
        )
        .exact_duplicate()
        .unwrap()
        .is_none()
    );
}
