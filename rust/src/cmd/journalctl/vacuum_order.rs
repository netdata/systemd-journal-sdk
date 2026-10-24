use super::VacuumCandidate;
use std::cmp::Ordering;

struct Run {
    next: usize,
    end: usize,
}

pub(super) struct VacuumOrder<'a> {
    candidates: &'a [VacuumCandidate],
    heads: Vec<Run>,
}

impl<'a> VacuumOrder<'a> {
    pub(super) fn new(candidates: &'a mut [VacuumCandidate]) -> Self {
        // A pairwise sequence/time comparator can cycle after a clock rollback.
        candidates.sort_unstable_by(|a, b| {
            b.have_seqno
                .cmp(&a.have_seqno)
                .then_with(|| {
                    if a.have_seqno {
                        a.seqnum_id.cmp(&b.seqnum_id).then(a.seqnum.cmp(&b.seqnum))
                    } else {
                        Ordering::Equal
                    }
                })
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut heads = Vec::new();
        let mut start = 0;
        while start < candidates.len() {
            let mut end = start + 1;
            if candidates[start].have_seqno {
                while end < candidates.len()
                    && candidates[end].have_seqno
                    && candidates[end].seqnum_id == candidates[start].seqnum_id
                {
                    end += 1;
                }
            }
            heads.push(Run { next: start, end });
            start = end;
        }
        let mut order = Self { candidates, heads };
        for i in (0..order.heads.len() / 2).rev() {
            order.down(i);
        }
        order
    }

    fn less(&self, i: usize, j: usize) -> bool {
        let a = &self.candidates[self.heads[i].next];
        let b = &self.candidates[self.heads[j].next];
        a.realtime
            .cmp(&b.realtime)
            .then(b.have_seqno.cmp(&a.have_seqno))
            .then_with(|| {
                if a.have_seqno {
                    a.seqnum_id.cmp(&b.seqnum_id)
                } else {
                    Ordering::Equal
                }
            })
            .then_with(|| a.name.cmp(&b.name))
            .is_lt()
    }

    fn down(&mut self, mut i: usize) {
        loop {
            let mut child = 2 * i + 1;
            if child >= self.heads.len() {
                return;
            }
            if child + 1 < self.heads.len() && self.less(child + 1, child) {
                child += 1;
            }
            if !self.less(child, i) {
                return;
            }
            self.heads.swap(i, child);
            i = child;
        }
    }
}

impl<'a> Iterator for VacuumOrder<'a> {
    type Item = &'a VacuumCandidate;

    fn next(&mut self) -> Option<Self::Item> {
        let candidate = &self.candidates[self.heads.first()?.next];
        self.heads[0].next += 1;
        if self.heads[0].next == self.heads[0].end {
            self.heads.swap_remove(0);
        }
        // The newly exposed timestamp may move backward; only the root changed.
        self.down(0);
        Some(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn history_order() {
        let mut rows: Vec<_> = [
            ('A', 1, 1, 300),
            ('B', 1, 2, 100),
            ('C', 2, 1, 200),
            ('D', 0, 0, 200),
            ('E', 1, 2, 50),
        ]
        .into_iter()
        .map(|(name, id, seqnum, realtime)| VacuumCandidate {
            name: name.to_string().into(),
            path: PathBuf::new(),
            usage: 0,
            have_seqno: id != 0,
            seqnum_id: [id; 16],
            seqnum,
            realtime,
        })
        .collect();
        fn permutations(rows: &mut [VacuumCandidate], i: usize) {
            if i < rows.len() {
                for j in i..rows.len() {
                    rows.swap(i, j);
                    permutations(rows, i + 1);
                    rows.swap(i, j);
                }
                return;
            }
            let mut copy = rows.to_vec();
            let names: Vec<_> = VacuumOrder::new(&mut copy)
                .map(|c| c.name.to_str().unwrap())
                .collect();
            assert_eq!(names, ["C", "D", "A", "B", "E"]);
        }
        permutations(&mut rows, 0);
        assert!(VacuumOrder::new(&mut []).next().is_none());
    }
}
