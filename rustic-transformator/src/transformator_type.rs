pub enum TransformatorType {
    SingleColumn { column_name: String },
    MultiColumn,
    NoOp,
}
