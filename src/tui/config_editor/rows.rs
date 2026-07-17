use super::fields::{FieldKind, SECTION_NAMES};

pub(super) enum Row {
    Section(usize),
    Item(usize, usize),
    Field {
        si: usize,
        ii: Option<usize>,
        fi: usize,
    },
    EmptyHint,
}

impl super::ConfigScreen {
    fn build_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (si, _) in SECTION_NAMES.iter().enumerate() {
            rows.push(Row::Section(si));
            if self.collapsed[si] {
                continue;
            }
            if self.is_array_section(si) {
                if self.array_len(si) == 0 {
                    rows.push(Row::EmptyHint);
                } else {
                    for ii in 0..self.array_len(si) {
                        rows.push(Row::Item(si, ii));
                        for fi in 0..self.field_count_for_section(si) {
                            rows.push(Row::Field {
                                si,
                                ii: Some(ii),
                                fi,
                            });
                        }
                    }
                }
            } else {
                for fi in 0..self.field_count_for_section(si) {
                    rows.push(Row::Field { si, ii: None, fi });
                }
            }
        }
        rows
    }

    pub(super) fn rebuild_rows(&mut self) {
        self.rows = self.build_rows();
        if self.row >= self.rows.len() {
            self.row = self.rows.len().saturating_sub(1);
        }
        while self.row > 0 && matches!(self.rows.get(self.row), Some(Row::EmptyHint)) {
            self.row -= 1;
        }
    }

    pub(super) fn field_at_cursor(
        &self,
    ) -> Option<(usize, Option<usize>, usize, String, FieldKind)> {
        let (si, ii, fi) = match self.rows.get(self.row)? {
            Row::Field { si, ii, fi } => (*si, *ii, *fi),
            _ => return None,
        };
        let &(name, kind) = self.fields_for_section(si).get(fi)?;
        Some((si, ii, fi, name.to_string(), kind))
    }

    pub(super) fn section_at_cursor(&self) -> Option<usize> {
        self.rows[..=self.row].iter().rev().find_map(|r| match r {
            Row::Section(si) => Some(*si),
            _ => None,
        })
    }

    pub(super) fn item_at_cursor(&self) -> Option<(usize, usize)> {
        self.rows[..=self.row].iter().rev().find_map(|r| match r {
            Row::Item(si, ii) => Some((*si, *ii)),
            _ => None,
        })
    }
}
