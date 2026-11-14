//! Bridges an immediately fetched first page to the SDK subscription. The subscription's
//! indices remain private until it catches up; published snapshots merge by event ID.

use crate::{EntryKind, TimelineDiff, TimelineEntry};
use std::collections::{HashMap, HashSet};

enum Phase {
    Waiting(Vec<TimelineEntry>),
    Showing { source: Vec<TimelineEntry>, first: Vec<TimelineEntry> },
    Live,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    Waiting,
    CatchingUp,
    Complete,
}

pub(crate) struct Bootstrap {
    phase: Phase,
    /// Keep a first-page row's key when the SDK gives that event its own entry ID.
    ids: HashMap<String, String>,
    /// Events supplied by the SDK, including ones removed in the same batch or before
    /// the direct response. Their early copies must never be restored.
    observed: HashSet<String>,
    root: String,
}

fn event_id(entry: &TimelineEntry) -> Option<&str> {
    match &entry.kind {
        EntryKind::Message(message) => Some(&message.event_id),
        _ => None,
    }
}

impl Bootstrap {
    pub fn progress(&self) -> Progress {
        match self.phase {
            Phase::Waiting(_) => Progress::Waiting,
            Phase::Showing { .. } => Progress::CatchingUp,
            Phase::Live => Progress::Complete,
        }
    }

    pub fn new(reset: &TimelineDiff, root: &str) -> Self {
        let source = match reset {
            TimelineDiff::Reset(entries) => entries.clone(),
            _ => Vec::new(),
        };
        let observed = source.iter().filter_map(event_id).map(ToOwned::to_owned).collect();
        Self { phase: Phase::Waiting(source), ids: HashMap::new(), observed, root: root.to_owned() }
    }

    pub fn update(&mut self, mut diffs: Vec<TimelineDiff>) -> Vec<TimelineDiff> {
        for diff in &mut diffs {
            let mut prepare = |entry: &mut TimelineEntry| {
                if !matches!(self.phase, Phase::Live)
                    && let Some(id) = event_id(entry)
                {
                    self.observed.insert(id.to_owned());
                }
                if let Phase::Showing { first, .. } = &self.phase
                    && let Some(id) = event_id(entry)
                    && let Some(early) = first.iter().find(|early| event_id(early) == Some(id))
                {
                    self.ids.entry(entry.id.clone()).or_insert_with(|| early.id.clone());
                }
                if let Some(id) = self.ids.get(&entry.id) {
                    entry.id = id.clone();
                }
            };
            match diff {
                TimelineDiff::Reset(entries) | TimelineDiff::Append(entries) => entries.iter_mut().for_each(prepare),
                TimelineDiff::PushFront(entry)
                | TimelineDiff::PushBack(entry)
                | TimelineDiff::Insert { entry, .. }
                | TimelineDiff::Set { entry, .. } => prepare(entry),
                _ => {}
            }
        }
        match &mut self.phase {
            Phase::Waiting(source) | Phase::Showing { source, .. } => apply(source, diffs.clone()),
            Phase::Live => return diffs,
        }
        self.snapshot().map_or(diffs, |entries| vec![TimelineDiff::Reset(entries)])
    }

    pub fn show(&mut self, mut first: Vec<TimelineEntry>) -> Option<TimelineDiff> {
        let Phase::Waiting(source) = &mut self.phase else { return None };
        for entry in &mut first {
            if let Some(existing) =
                source.iter().find(|known| event_id(known).is_some() && event_id(known) == event_id(entry))
            {
                entry.id = existing.id.clone();
            }
        }
        self.phase = Phase::Showing { source: std::mem::take(source), first };
        self.snapshot().map(TimelineDiff::Reset)
    }

    /// A failed direct request has contributed no rows; ordinary SDK diffs can flow through.
    pub fn cancel(&mut self) {
        if matches!(self.phase, Phase::Waiting(_)) {
            self.phase = Phase::Live;
            self.observed.clear();
        }
    }

    fn snapshot(&mut self) -> Option<Vec<TimelineEntry>> {
        let Phase::Showing { source, first } = &self.phase else { return None };
        // Only an applied subscription entry acknowledges an early reply. A completed network
        // page is neither a subscription barrier nor proof that it covered 30 thread replies:
        // the SDK's recursive page also includes reactions and edits.
        let mut entries = source.clone();
        let mut missing = false;
        for (index, early) in first.iter().enumerate().rev() {
            if event_id(early).is_none_or(|id| self.observed.contains(id)) {
                continue;
            }
            missing = true;
            // Shared event IDs anchor the server's order. With no shared reply yet, replies
            // follow the root and precede live events received during the request.
            let position = |known: &TimelineEntry| {
                entries.iter().position(|entry| event_id(entry).is_some() && event_id(entry) == event_id(known))
            };
            let next = first[index + 1..].iter().find_map(position);
            let previous = first[..index]
                .iter()
                .rev()
                .find_map(position)
                .or_else(|| entries.iter().position(|entry| event_id(entry) == Some(self.root.as_str())));
            let at = next.or_else(|| previous.map(|at| at + 1)).unwrap_or(0);
            entries.insert(at, early.clone());
        }
        if !missing {
            self.phase = Phase::Live;
            self.observed.clear();
        }
        Some(entries)
    }
}

fn apply(entries: &mut Vec<TimelineEntry>, diffs: Vec<TimelineDiff>) {
    for diff in diffs {
        match diff {
            TimelineDiff::Reset(all) => *entries = all,
            TimelineDiff::Append(more) => entries.extend(more),
            TimelineDiff::Clear => entries.clear(),
            TimelineDiff::PushFront(entry) => entries.insert(0, entry),
            TimelineDiff::PushBack(entry) => entries.push(entry),
            TimelineDiff::PopFront if !entries.is_empty() => {
                entries.remove(0);
            }
            TimelineDiff::PopBack => {
                entries.pop();
            }
            TimelineDiff::Insert { index, entry } if index <= entries.len() => entries.insert(index, entry),
            TimelineDiff::Set { index, entry } if index < entries.len() => entries[index] = entry,
            TimelineDiff::Remove { index } if index < entries.len() => {
                entries.remove(index);
            }
            TimelineDiff::Truncate { length } => entries.truncate(length),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
