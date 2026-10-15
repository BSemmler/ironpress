//! The shaping view a parsed font owns over its own bytes.
//!
//! `rustybuzz::Face` borrows the font buffer it was built from, so keeping a
//! face alive means keeping that buffer alive. Storing the two together in one
//! value is what lets a parsed font hold its face outright, instead of a
//! process-wide table handing out faces over a buffer nothing owns.

use std::fmt;
use std::sync::{Arc, RwLock};

use self_cell::self_cell;

/// The shaping tables rustybuzz derives from a font's bytes.
type Face<'a> = rustybuzz::Face<'a>;

self_cell!(
    /// A `rustybuzz::Face` together with the buffer it reads.
    ///
    /// Building a face parses the font's shaping tables. Layout shapes every
    /// run at least twice — once to measure, again to paint — so deriving the
    /// face per run is the dominant cost of text. Deriving it once with the
    /// font removes that cost without any lifetime extension: the face lives
    /// exactly as long as this value, and this value's owner keeps its bytes.
    struct FaceCell {
        owner: Arc<Vec<u8>>,

        #[covariant]
        dependent: Face,
    }
);

/// Shape plans kept per face. A face sees only a handful of direction, script,
/// language and feature combinations, so this stays small; the cap only guards
/// against unusual documents.
const MAX_CACHED_PLANS: usize = 32;

/// The shaping tables for one face, plus the shape plans built for it.
///
/// Building a `rustybuzz::ShapePlan` resolves the face's OpenType features for a
/// script and language, and `rustybuzz::shape` does that afresh on every call. A
/// document shapes thousands of runs in the same few configurations, so the plan
/// is built once per configuration and reused. Plans live with their face, so a
/// plan can never be applied to a different face.
pub(crate) struct ShapingFace {
    cell: FaceCell,
    plans: RwLock<Vec<(PlanKey, Arc<rustybuzz::ShapePlan>)>>,
}

/// Everything a shape plan depends on besides the face
#[derive(PartialEq)]
struct PlanKey {
    direction: rustybuzz::Direction,
    script: rustybuzz::Script,
    language: Option<rustybuzz::Language>,
    features: Vec<rustybuzz::Feature>,
}

impl ShapingFace {
    /// Build the shaping face for `face_index` within `data`.
    ///
    /// `None` means rustybuzz rejected the bytes. Shaping then declines the run
    /// rather than substituting another face, which is the same outcome a
    /// failed `Face::from_slice` produced when each run built its own face.
    pub(crate) fn parse(data: Arc<Vec<u8>>, face_index: u32) -> Option<Self> {
        let cell = FaceCell::try_new(data, |data| {
            rustybuzz::Face::from_slice(data.as_slice(), face_index).ok_or(())
        })
        .ok()?;
        Some(Self {
            cell,
            plans: RwLock::default(),
        })
    }

    /// The shaping face, borrowed for as long as this value lives.
    pub(crate) fn face(&self) -> &rustybuzz::Face<'_> {
        self.cell.borrow_dependent()
    }

    /// Shapes `buffer` exactly as `rustybuzz::shape` would, reusing this face's
    /// plan for the buffer's direction, script, language and `features`.
    pub(crate) fn shape(
        &self,
        features: &[rustybuzz::Feature],
        mut buffer: rustybuzz::UnicodeBuffer,
    ) -> rustybuzz::GlyphBuffer {
        buffer.guess_segment_properties();
        let script = buffer.script();
        // `UnicodeBuffer::script` reports an unset script as UNKNOWN, but a plan
        // built for UNKNOWN is not the plan `shape` builds for no script, so
        // leave that rare case uncached
        if script == rustybuzz::script::UNKNOWN {
            return rustybuzz::shape(self.face(), features, buffer);
        }
        let key = PlanKey {
            direction: buffer.direction(),
            script,
            language: buffer.language(),
            features: features.to_vec(),
        };
        let plan = self.plan(key);
        rustybuzz::shape_with_plan(self.face(), &plan, buffer)
    }

    fn plan(&self, key: PlanKey) -> Arc<rustybuzz::ShapePlan> {
        let cached = |plans: &[(PlanKey, Arc<rustybuzz::ShapePlan>)], key: &PlanKey| {
            plans
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, plan)| Arc::clone(plan))
        };
        if let Some(plan) = cached(&self.plans.read().unwrap_or_else(|e| e.into_inner()), &key) {
            return plan;
        }
        let plan = Arc::new(rustybuzz::ShapePlan::new(
            self.face(),
            key.direction,
            Some(key.script),
            key.language.as_ref(),
            &key.features,
        ));
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        // Another thread may have built the same plan meanwhile; keep the first
        if let Some(existing) = cached(&plans, &key) {
            return existing;
        }
        if plans.len() < MAX_CACHED_PLANS {
            plans.push((key, Arc::clone(&plan)));
        }
        plan
    }
}

impl fmt::Debug for ShapingFace {
    /// `rustybuzz::Face` is not `Debug`, and dumping its parsed tables would
    /// bury the surrounding font. The buffer size is the part that identifies
    /// which face a font is carrying.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShapingFace")
            .field("bytes", &self.cell.borrow_owner().len())
            .finish()
    }
}
