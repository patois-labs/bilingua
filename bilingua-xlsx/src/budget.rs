//! The spreadsheet limits (`xlsx_max_cells`, `xlsx_max_rows`, `xlsx_max_cell_chars`, `xlsx_max_total_text_chars`)
//! as one running count. The native walk and the LocOS-profile walk (cell mode and column mode) both charge it, so
//! the two cannot drift: a workbook one refuses for its size, the other refuses too.

use bilingua_model::{Error, LimitError, LimitKind, Limits};

pub(crate) struct SheetBudget {
    limits: Limits,
    /// Cells seen so far, for the cell limit.
    cells: usize,
    /// Characters of cell text seen so far (a shared string counts once for the table and once per cell using it).
    text_chars: usize,
}

fn refusal(msg: String) -> Error {
    LimitError::new(LimitKind::Spreadsheet, msg).into()
}

impl SheetBudget {
    pub(crate) fn new(limits: Limits) -> SheetBudget {
        SheetBudget { limits, cells: 0, text_chars: 0 }
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }

    /// A row with this number: refused past `xlsx_max_rows`.
    pub(crate) fn row(&self, row: i64) -> Result<(), Error> {
        if row > i64::from(self.limits.xlsx_max_rows) {
            return Err(refusal(format!("This file has a row past row {}. Split it into smaller files.", self.limits.xlsx_max_rows)));
        }
        Ok(())
    }

    /// One more cell: refused past `xlsx_max_cells`.
    pub(crate) fn cell(&mut self) -> Result<(), Error> {
        self.cells += 1;
        if self.cells > self.limits.xlsx_max_cells {
            return Err(refusal(format!("This file has more than {} cells. Split it into smaller files.", self.limits.xlsx_max_cells)));
        }
        Ok(())
    }

    /// A cell text of `chars` characters, counted `times` times: refused over the per-cell limit or once the total
    /// passes `xlsx_max_total_text_chars`. Call it before the text is copied.
    pub(crate) fn text(&mut self, chars: usize, times: usize) -> Result<(), Error> {
        self.text_chars = self.text_chars.saturating_add(chars.saturating_mul(times));
        if chars > self.limits.xlsx_max_cell_chars {
            return Err(refusal(format!("This file has a cell with more than {} characters. Split it into smaller files.", self.limits.xlsx_max_cell_chars)));
        }
        if self.text_chars > self.limits.xlsx_max_total_text_chars {
            return Err(refusal(format!(
                "This file has more than {} characters of cell text (repeated shared strings included). Split it into smaller files.",
                self.limits.xlsx_max_total_text_chars
            )));
        }
        Ok(())
    }
}
