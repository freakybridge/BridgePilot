//! Conservative equality for reusing an already verified Windows installation.
//! This is never an integrity digest and never authorizes replacing either file.
use std::ops::Range;

fn u16_at(body: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(body.get(at..at.checked_add(2)?)?.try_into().ok()?) as usize)
}
fn u32_at(body: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(body.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize)
}

fn metadata(body: &[u8]) -> Option<Vec<Range<usize>>> {
    if body.get(..2)? != b"MZ" { return None; }
    let pe = u32_at(body, 0x3c)?;
    if pe < 0x40 || body.get(pe..pe.checked_add(4)?)? != b"PE\0\0" { return None; }
    let count = u16_at(body, pe + 6)?;
    if !(1..=96).contains(&count) { return None; }
    let optional = pe.checked_add(24)?;
    let table = optional.checked_add(u16_at(body, pe + 20)?)?;
    let header_end = table.checked_add(count.checked_mul(40)?)?;
    body.get(..header_end)?;
    let (directories, number) = match u16_at(body, optional)? {
        0x20b => (optional + 112, optional + 108),
        0x10b => (optional + 96, optional + 92),
        _ => return None,
    };
    if directories + 56 > table || u32_at(body, number)? <= 6 { return None; }
    // Raw sections must be bounded, disjoint and outside the headers. Debug
    // metadata must additionally live entirely in a non-executable section.
    let mut sections = Vec::new();
    for index in 0..count {
        let at = table + index * 40;
        let size = u32_at(body, at + 16)?;
        let start = u32_at(body, at + 20)?;
        if size == 0 { continue; }
        let end = start.checked_add(size)?;
        body.get(start..end)?;
        if start < header_end || sections.iter().any(|(_, range, _): &(usize, Range<usize>, bool)| start < range.end && range.start < end) {
            return None;
        }
        sections.push((u32_at(body, at + 12)?, start..end, u32_at(body, at + 36)? & 0x2000_0000 != 0));
    }
    let debug_rva = u32_at(body, directories + 48)?;
    let debug_size = u32_at(body, directories + 52)?;
    if debug_size == 0 || debug_size % 28 != 0 || debug_size > 28 * 4096 { return None; }
    let map = |rva: usize, size: usize| -> Option<Range<usize>> {
        let mut found = None;
        for (base, range, executable) in &sections {
            let Some(delta) = rva.checked_sub(*base) else { continue; };
            let end = delta.checked_add(size)?;
            if end <= range.len() {
                if *executable || found.is_some() { return None; }
                found = Some(range.start + delta..range.start + end);
            }
        }
        found
    };
    let debug = map(debug_rva, debug_size)?;
    let mut excluded = vec![pe + 8..pe + 12];
    for at in (debug.start..debug.end).step_by(28) {
        excluded.push(at + 4..at + 8);
        if u32_at(body, at + 12)? == 2 {
            let size = u32_at(body, at + 16)?;
            if size < 25 { return None; }
            let cv = map(u32_at(body, at + 20)?, size)?;
            if cv.start != u32_at(body, at + 24)? || cv.start < debug.end && debug.start < cv.end
                || body.get(cv.start..cv.start + 4)? != b"RSDS"
                || !body.get(cv.start + 24..cv.end)?.contains(&0) {
                return None;
            }
            excluded.push(cv.start + 4..cv.start + 20);
        }
    }
    excluded.sort_by_key(|range| range.start);
    if excluded.windows(2).any(|pair| pair[0].end > pair[1].start) { return None; }
    Some(excluded)
}

pub(crate) fn equivalent(left: &[u8], right: &[u8]) -> bool {
    if left == right { return true; }
    if left.len() != right.len() { return false; }
    let Some(excluded) = metadata(left) else { return false; };
    if metadata(right).as_ref() != Some(&excluded) { return false; }
    let mut start = 0;
    for range in excluded {
        if left[start..range.start] != right[start..range.start] { return false; }
        start = range.end;
    }
    left[start..] == right[start..]
}

#[cfg(test)]
#[path = "../../../scripts/tests/orchestrator/pe_equivalence.rs"]
mod tests;
