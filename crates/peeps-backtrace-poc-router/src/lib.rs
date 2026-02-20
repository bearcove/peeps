use peeps_backtrace_poc_leaf::{pipeline::stage_one::stage_two, CapturedTrace};

#[inline(never)]
pub fn alpha_path() -> CapturedTrace {
    alpha::entry()
}

#[inline(never)]
pub fn beta_path() -> CapturedTrace {
    beta::entry()
}

mod alpha {
    use super::*;

    #[inline(never)]
    pub fn entry() -> CapturedTrace {
        hop()
    }

    #[inline(never)]
    fn hop() -> CapturedTrace {
        stage_two::collect_here("alpha_path")
    }
}

mod beta {
    use super::*;

    #[inline(never)]
    pub fn entry() -> CapturedTrace {
        first()
    }

    #[inline(never)]
    fn first() -> CapturedTrace {
        second()
    }

    #[inline(never)]
    fn second() -> CapturedTrace {
        stage_two::collect_here("beta_path")
    }
}
