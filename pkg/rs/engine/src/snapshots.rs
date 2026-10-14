//! Context snapshots: immutable checkpoints, checkpoint-plus-delta
//! chains, the scan index, and the restore that yields a context
//! without replaying any preceding control.

use kdlith_diagnostic::DiagnosticRecord;
use kdlith_runtime::{Context, ContextError, ContextLayout, SlotId, SlotValue};

/// The code a rejected restore reports under.
pub const SNAPSHOT_MISMATCH: &str = "KDLITH-CTX004";

/// Why a snapshot could not be restored.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotError {
    /// The snapshot's schema fingerprint differs from the requesting
    /// definition's.
    Fingerprint {
        snapshot: String,
        requesting: String,
    },
    /// The snapshot's invocation lineage differs from the requesting
    /// invocation's.
    Lineage {
        snapshot: String,
        requesting: String,
    },
    /// The snapshot's materialized values do not fit the requesting
    /// context layout.
    Layout(String),
}

impl SnapshotError {
    /// The registered code the failure reports under.
    pub fn code(&self) -> &'static str {
        SNAPSHOT_MISMATCH
    }

    /// The diagnostic record a caller surfaces for the failure.
    pub fn diagnostic(&self, snapshot: &str) -> DiagnosticRecord {
        let reason = match self {
            SnapshotError::Fingerprint {
                snapshot,
                requesting,
            } => format!(
                "its schema fingerprint is {snapshot} but the requesting definition's is {requesting}"
            ),
            SnapshotError::Lineage {
                snapshot,
                requesting,
            } => format!(
                "its invocation lineage is [{snapshot}] but the requesting invocation's is [{requesting}]"
            ),
            SnapshotError::Layout(reason) => reason.clone(),
        };
        DiagnosticRecord::from_code(SNAPSHOT_MISMATCH)
            .expect("KDLITH-CTX004 is registered and never retired")
            .with_message(format!("snapshot {snapshot} cannot restore here: {reason}"))
            .with_origin("engine")
    }
}

impl From<ContextError> for SnapshotError {
    fn from(error: ContextError) -> Self {
        SnapshotError::Layout(error.to_string())
    }
}

/// An immutable snapshot of one context: a full checkpoint, or a delta
/// against an earlier snapshot in the same chain. Snapshots are shared
/// by cloning; nothing mutates one after capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextSnapshot {
    id: String,
    /// The fingerprint of the schema the captured context processed.
    fingerprint: String,
    /// The chain of invocations the capture happened in, outermost first.
    lineage: Vec<String>,
    /// The chain this snapshot extends; `None` is a full checkpoint.
    base: Option<Box<ContextSnapshot>>,
    /// The slots a delta node records, in slot order. Empty for a full
    /// checkpoint, whose values are held directly.
    delta: Vec<(SlotId, Option<SlotValue>)>,
    values: Vec<Option<SlotValue>>,
}

impl ContextSnapshot {
    /// Capture a full checkpoint of `context`.
    pub fn checkpoint(
        id: impl Into<String>,
        context: &Context,
        fingerprint: impl Into<String>,
        lineage: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            fingerprint: fingerprint.into(),
            lineage,
            base: None,
            delta: Vec::new(),
            values: context.snapshot_values(),
        }
    }

    /// Capture a delta against `base`: only the slots whose values
    /// differ from the chain's materialized state. The base must share
    /// this capture's fingerprint and lineage.
    pub fn delta(
        id: impl Into<String>,
        base: &ContextSnapshot,
        context: &Context,
    ) -> Result<Self, SnapshotError> {
        let materialized = base.materialize();
        let current = context.snapshot_values();
        if materialized.len() != current.len() {
            return Err(SnapshotError::Layout(format!(
                "the context holds {} slots but the chain holds {}",
                current.len(),
                materialized.len()
            )));
        }
        let delta = context
            .layout()
            .slots()
            .enumerate()
            .filter_map(|(at, slot)| {
                let value = current.get(at)?;
                (materialized.get(at) != Some(value)).then_some((slot, value.clone()))
            })
            .collect();
        Ok(Self {
            id: id.into(),
            fingerprint: base.fingerprint.clone(),
            lineage: base.lineage.clone(),
            base: Some(Box::new(base.clone())),
            delta,
            values: Vec::new(),
        })
    }

    /// The snapshot's identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The schema fingerprint the snapshot was captured under.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// The invocation lineage the snapshot was captured in.
    pub fn lineage(&self) -> &[String] {
        &self.lineage
    }

    /// The materialized slot values of the whole chain, in slot order.
    pub fn materialize(&self) -> Vec<Option<SlotValue>> {
        let mut values = match &self.base {
            Some(base) => base.materialize(),
            None => self.values.clone(),
        };
        for (slot, value) in &self.delta {
            if let Some(at) = values.get_mut(slot.index()) {
                *at = value.clone();
            }
        }
        values
    }

    /// Restore the snapshot into a context over `layout`, without
    /// replaying any preceding control. The requesting fingerprint and
    /// lineage must match the snapshot's exactly; a mismatch is
    /// `KDLITH-CTX004`, never a best-effort restore.
    pub fn restore(
        &self,
        layout: &ContextLayout,
        fingerprint: &str,
        lineage: &[String],
    ) -> Result<Context, SnapshotError> {
        if self.fingerprint != fingerprint {
            return Err(SnapshotError::Fingerprint {
                snapshot: self.fingerprint.clone(),
                requesting: fingerprint.to_owned(),
            });
        }
        if self.lineage != lineage {
            return Err(SnapshotError::Lineage {
                snapshot: self.lineage.join(","),
                requesting: lineage.join(","),
            });
        }
        let values = self.materialize();
        if values.len() != layout.slot_count() {
            return Err(SnapshotError::Layout(format!(
                "the snapshot holds {} slots but the layout declares {}",
                values.len(),
                layout.slot_count()
            )));
        }
        Ok(Context::from_values(layout.clone(), values)?)
    }
}

/// How far an indexed record's checking has progressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationStatus {
    /// The record was bound and validated.
    Validated,
    /// The record holds a pending binding awaiting its declaration.
    Pending,
    /// The record failed validation.
    Failed,
}

impl ValidationStatus {
    pub fn name(self) -> &'static str {
        match self {
            ValidationStatus::Validated => "validated",
            ValidationStatus::Pending => "pending",
            ValidationStatus::Failed => "failed",
        }
    }
}

/// One entry of the index the engine writes while scanning, so a record
/// can be resolved later by restoring its context snapshot and binding
/// its recorded span instead of replaying every preceding control.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SnapshotIndexEntry {
    /// The indexed record's identity.
    pub identity: String,
    /// The record's type.
    pub ty: String,
    /// The source digest the record was read from.
    pub digest: String,
    /// The record's span in that source.
    pub span: String,
    /// The schema fingerprint the scan ran under.
    pub schema_fingerprint: String,
    /// The snapshot restoring the record's context.
    pub snapshot: String,
    /// The invocation the scan ran in.
    pub invocation: String,
    /// How far the record's checking has progressed.
    pub validation: ValidationStatus,
}

/// The index of one scan: one entry per indexed record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotIndex {
    entries: Vec<SnapshotIndexEntry>,
}

impl SnapshotIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one indexed record.
    pub fn record(&mut self, entry: SnapshotIndexEntry) {
        self.entries.push(entry);
    }

    /// The entries in scan order.
    pub fn entries(&self) -> &[SnapshotIndexEntry] {
        &self.entries
    }

    /// The entry naming `snapshot`, when the index holds one.
    pub fn by_snapshot(&self, snapshot: &str) -> Option<&SnapshotIndexEntry> {
        self.entries.iter().find(|e| e.snapshot == snapshot)
    }
}
