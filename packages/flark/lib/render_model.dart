/// The flat render model as the parse crate emits it, with the schema
/// constants. Hosts that paint from a [Projection] rarely need this; tests
/// and advanced consumers read it directly.
library;

export 'src/parse/render_model.dart';
export 'src/parse/schema.g.dart';
