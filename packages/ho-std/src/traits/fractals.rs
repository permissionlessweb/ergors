pub trait FractalRequirementsExt {
    type FractalRequirements;
    fn new_default() -> Self::FractalRequirements;
}

pub trait CosmicContextExt {
    type CosmicContext;
    fn new_context(task_id: String, prompt: &str, recursion_depth: u32) -> Self::CosmicContext;
}
