// SPDX-License-Identifier: Apache-2.0

mod documents;
mod error;
mod logging;
mod params;

use anyhow::{Context, Result};
use structopt::StructOpt;

fn main() -> Result<()> {
    logging::try_init("trace").context("failed to initialize logger")?;

    let params = params::Params::from_args();

    let absolute_doc_path = if params.doc_path.is_relative() {
        std::env::current_dir()?.join(params.doc_path)
    } else {
        params.doc_path
    };

    let mut documents = documents::Documents::new(absolute_doc_path)?;
    documents.parse()?;
    documents.sync()?;

    Ok(())
}
