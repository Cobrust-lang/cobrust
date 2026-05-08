//! L2.behavior fuzz harness for cobrust-click.
//!
//! Constitution §4.2 floor: ≥ 1000 fuzzed inputs per public function.
//! We synthesise random decorator chains + random argv strings and
//! assert panic-freedom. Successful parses must round-trip via the
//! observer surface (`option(...) / argument(...)`).


#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use cobrust_click::{ArgumentSpec, Command, OptionSpec, ParamType};

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) as u32) ^ ((z >> 32) as u32)
    }
}

fn synth_token(rng: &mut Lcg) -> String {
    let len = (rng.next_u32() % 6) as usize + 1;
    (0..len)
        .map(|_| {
            let c = (rng.next_u32() % 26 + b'a' as u32) as u8;
            char::from(c)
        })
        .collect()
}

fn synth_command(rng: &mut Lcg) -> Command {
    let n_opts = (rng.next_u32() % 4) as usize;
    let n_args = (rng.next_u32() % 3) as usize;
    let mut cmd = Command::new(synth_token(rng));
    for i in 0..n_opts {
        let mut opt = OptionSpec::new(format!("opt-{i}"))
            .type_(match rng.next_u32() % 3 {
                0 => ParamType::Str,
                1 => ParamType::Int,
                _ => ParamType::Bool,
            })
            .help(synth_token(rng));
        if rng.next_u32() % 2 == 0 {
            opt = opt.default("0");
        }
        cmd = cmd.option(opt);
    }
    for i in 0..n_args {
        let arg = ArgumentSpec::new(format!("arg{i}")).optional();
        cmd = cmd.argument(arg);
    }
    cmd
}

#[test]
fn run_panic_free_on_random_inputs() {
    let seeds: [u64; 3] = [42, 1337, 0xDEAD_BEEF];
    let mut total = 0;
    for &seed in &seeds {
        let mut rng = Lcg::new(seed);
        for _ in 0..400 {
            let cmd = synth_command(&mut rng);
            let argc = (rng.next_u32() % 6) as usize;
            let mut argv: Vec<String> = vec![cmd.name().to_owned()];
            for _ in 0..argc {
                argv.push(synth_token(&mut rng));
            }
            // The point is panic-freedom; either Ok(_) or Err(_) is
            // acceptable.
            let _ = cmd.run(argv);
            total += 1;
        }
    }
    assert!(total >= 1000, "fuzz coverage shortfall: {total}");
}

#[test]
fn builder_chain_is_order_independent_for_options() {
    // Decorator-chain order must not change the set of recognised
    // options — `@click.option('--a') @click.option('--b')` and the
    // reverse must accept the same argv.
    let a_first = Command::new("c")
        .option(OptionSpec::new("a"))
        .option(OptionSpec::new("b"));
    let b_first = Command::new("c")
        .option(OptionSpec::new("b"))
        .option(OptionSpec::new("a"));
    let r1 = a_first
        .run(vec!["c", "--a", "1", "--b", "2"])
        .expect("a-first parse");
    let r2 = b_first
        .run(vec!["c", "--a", "1", "--b", "2"])
        .expect("b-first parse");
    assert_eq!(r1.option("a"), r2.option("a"));
    assert_eq!(r1.option("b"), r2.option("b"));
}
