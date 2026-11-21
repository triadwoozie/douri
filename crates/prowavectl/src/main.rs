//! prowavectl: command-line client and MCP server for Pro Wave's control API.

fn main() {
    std::process::exit(prowavectl::cli::run());
}
