use dumpster::unsync::Gc;
use thyme::{Host, Runtime, eval::Evaluator};

fn main() {
    use std::io::Write;

    let runtime = Runtime::new();
    struct DummyHost;

    impl Host for DummyHost {
        fn resolve_property(
            &mut self,
            object: thyme::NativeObjectId,
            name: &str,
        ) -> Result<Option<thyme::PropertyId>, thyme::HostError> {
            todo!()
        }

        fn get_property(
            &mut self,
            object: thyme::NativeObjectId,
            property: thyme::PropertyId,
        ) -> Result<thyme::Value, thyme::HostError> {
            todo!()
        }

        fn set_property(
            &mut self,
            object: thyme::NativeObjectId,
            property: thyme::PropertyId,
            value: &thyme::Value,
        ) -> Result<(), thyme::HostError> {
            todo!()
        }

        fn call_intrinsic(
            &mut self,
            intrinsic: thyme::IntrinsicId,
            arguments: &[thyme::Value],
        ) -> Result<thyme::Value, thyme::HostError> {
            todo!()
        }
    }

    let mut host = DummyHost;

    let mut evaluator = Evaluator {
        runtime: &runtime,
        host: &mut host,
    };

    let env = Gc::new(thyme::Environment::new_root());

    let mut input = String::new();
    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_line(&mut input).unwrap();

        let trimmed = input.trim();
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }

        match thyme::parse::parse_thyme(trimmed).into_result() {
            Ok((expr, _)) => match evaluator.eval_expr(&expr, &env) {
                Ok(out) => println!("{out}"),
                Err(err) => println!("- ERROR - {err}"),
            },
            Err(errs) => {
                use ariadne::{Color, Label, Report, ReportKind, Source};
                for err in errs {
                    Report::build(ReportKind::Error, ((), err.span().into_range()))
                        .with_config(
                            ariadne::Config::new().with_index_type(ariadne::IndexType::Byte),
                        )
                        .with_code(3)
                        .with_message(err.to_string())
                        .with_label(
                            Label::new(((), err.span().into_range()))
                                .with_message(err.reason().to_string())
                                .with_color(Color::Red),
                        )
                        .finish()
                        .eprint(Source::from(trimmed))
                        .unwrap();
                }
            }
        }

        input.clear();
    }
}
