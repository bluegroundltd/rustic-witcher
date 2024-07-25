use rustic_base_transformations::noop_transformator::NoOpTransformator;
use rustic_transformator::transformator::Transformator;

pub trait WholeTableTransformator {
    fn transform(&self, operation_type_raw: &str) -> Box<dyn Transformator>;
}

pub struct NoOpWholeTableTransformator;

impl NoOpWholeTableTransformator {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl WholeTableTransformator for NoOpWholeTableTransformator {
    fn transform(&self, _: &str) -> Box<dyn Transformator> {
        Box::new(NoOpTransformator::new())
    }
}
