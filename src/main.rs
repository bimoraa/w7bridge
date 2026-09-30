use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {

    match w7bridge::run().await {

        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {

            eprintln!("오류: {error}");
            ExitCode::FAILURE

        }

    }

}
