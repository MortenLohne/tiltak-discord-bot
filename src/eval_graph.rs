use std::{
    io::{self, Write},
    mem,
    process::{Command, ExitStatus, Stdio},
};

// Use external python program to render a pretty graph of the game's eval
// Slightly modified version of the rendering code from WilemBot https://github.com/ViliamVadocz/tak/blob/main/graph.py
pub fn generate_graph(ptn: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut child = Command::new("python3")
        .arg("graph.py")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or(io::Error::other("Failed to open stdin"))?;

    stdin.write_all(ptn)?;
    mem::drop(stdin); // Close stdin file handle

    let output = child.wait_with_output()?;
    if !ExitStatus::success(&output.status) {
        Err(io::Error::other(format!(
            "Got exit status {}",
            output.status
        )))
    } else {
        Ok(output.stdout)
    }
}
