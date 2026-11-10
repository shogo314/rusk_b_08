//! Command-line syntax. Compilation and source loading live in their own layers.
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "rusk",
    version,
    about = "Compile Rusk programs to C++20 or inspect intermediate representations",
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Generate C++20 source or dump an intermediate representation
    #[command(args_override_self = true)]
    Compile(CompileArgs),
    /// List the Rusk sources and C++ resources loaded by a program
    Dependencies {
        /// Rusk source file
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Args)]
pub(crate) struct CompileArgs {
    /// Rusk source file
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
    /// Output file (the default applies to every emission stage)
    #[arg(short, long, value_name = "OUTPUT", default_value = "a.cpp")]
    pub output: PathBuf,
    /// Disable backend optimizations for comparison
    #[arg(long)]
    pub no_opt: bool,
    /// Representation to emit
    #[arg(long, value_enum, value_name = "STAGE", default_value = "cpp")]
    pub emit: EmitStage,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum EmitStage {
    Cst,
    Ast,
    Hir,
    Lir,
    Cpp,
}
