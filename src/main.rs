use clap::Parser;

#[derive(Parser)]
#[command(name = "blixard")]
#[command(about = "Distributed microVM orchestration platform", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Node operations
    Node {
        #[arg(long)]
        id: u64,
    },
    /// VM operations
    Vm {
        #[command(subcommand)]
        command: VmCommands,
    },
}

#[derive(clap::Subcommand)]
enum VmCommands {
    /// Create a new VM
    Create {
        #[arg(long)]
        name: String,
    },
    /// List all VMs
    List,
}

fn main() {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Node { id } => {
            eprintln!("Node functionality not yet implemented (id: {})", id);
            std::process::exit(1);
        }
        Commands::Vm { command } => match command {
            VmCommands::Create { name } => {
                eprintln!("VM creation not yet implemented (name: {})", name);
                std::process::exit(1);
            }
            VmCommands::List => {
                eprintln!("VM listing not yet implemented");
                std::process::exit(1);
            }
        },
    }
}