pub use peeps_source::SourceId;
pub use peeps_source::{intern_source, source_for_id};

use crate::caller_source;

#[track_caller]
pub(crate) fn caller_source_id() -> SourceId {
    intern_source(peeps_source::Source::new(caller_source(), None))
}
