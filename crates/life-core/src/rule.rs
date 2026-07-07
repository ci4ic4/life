#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BS { pub birth: u16, pub survive: u16 }

impl BS {
    pub fn born(&self, n: u8) -> bool { n <= 8 && self.birth & (1 << n) != 0 }
    pub fn survives(&self, n: u8) -> bool { n <= 8 && self.survive & (1 << n) != 0 }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RuleErr { BadFormat, BadDigit(char) }

/// Parse a digit-set rule like "B3/S23" (case-insensitive, B/S order fixed).
pub fn parse_bs(s: &str) -> Result<BS, RuleErr> {
    let s = s.trim();
    let (b_part, s_part) = s.split_once('/').ok_or(RuleErr::BadFormat)?;
    let digits = |part: &str, tag: char| -> Result<u16, RuleErr> {
        let mut bytes = part.chars();
        match bytes.next() {
            Some(c) if c.eq_ignore_ascii_case(&tag) => {}
            _ => return Err(RuleErr::BadFormat),
        }
        let mut mask = 0u16;
        for c in bytes {
            let d = c.to_digit(10).filter(|d| *d <= 8).ok_or(RuleErr::BadDigit(c))?;
            mask |= 1 << d;
        }
        Ok(mask)
    };
    Ok(BS { birth: digits(b_part, 'B')?, survive: digits(s_part, 'S')? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_conway() {
        let bs = parse_bs("B3/S23").unwrap();
        assert!(bs.born(3));
        assert!(!bs.born(2));
        assert!(bs.survives(2) && bs.survives(3));
        assert!(!bs.survives(1) && !bs.survives(4));
    }
    #[test]
    fn case_insensitive() {
        assert_eq!(parse_bs("b3/s23"), parse_bs("B3/S23"));
    }
    #[test]
    fn empty_sets_ok() {
        let bs = parse_bs("B/S").unwrap();
        assert_eq!(bs, BS { birth: 0, survive: 0 });
    }
    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_bs("3/23"), Err(RuleErr::BadFormat));
        assert_eq!(parse_bs("B9/S2"), Err(RuleErr::BadDigit('9')));
    }
}
