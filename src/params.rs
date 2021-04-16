// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use structopt::StructOpt;

/// Syncs source code to markdown documentation
#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Params {
    /// Path to file or folder with the markdown documentation to sync
    #[structopt(parse(from_os_str))]
    pub doc_path: PathBuf,
}
