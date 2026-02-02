// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, rename_all = "kebab")]
pub struct Params {
    /// Path to file or folder with the markdown documentation to sync
    pub doc_path: PathBuf,
}
