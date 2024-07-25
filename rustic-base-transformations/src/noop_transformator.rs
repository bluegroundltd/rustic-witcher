use polars::frame::DataFrame;
use rand::rngs::StdRng;
use rustic_transformator::transformator::Transformator;
use rustic_transformator::transformator_output::TransformatorOutput;
use rustic_transformator::transformator_type::TransformatorType;

pub struct NoOpTransformator;

impl NoOpTransformator {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Transformator for NoOpTransformator {
    fn transform(&self, _: &DataFrame, _: &mut StdRng) -> Vec<TransformatorOutput> {
        vec![]
    }

    fn transformator_type(&self) -> TransformatorType {
        TransformatorType::NoOp
    }
}
