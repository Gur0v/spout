use anyhow::Result;
use vergen_git2::{Emitter, Git2Builder};

fn main() -> Result<()> {
    let git2 = Git2Builder::default()
        .sha(true)
        .branch(true)
        .commit_date(true)
        .build()?;

    Emitter::default()
        .add_instructions(&git2)?
        .emit()?;

    Ok(())
}
