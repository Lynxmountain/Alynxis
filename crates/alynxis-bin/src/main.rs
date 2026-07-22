//! Alynxis executable entry point.
//!
//! Part 1: startup sequence. Part 2: WorldModel + `ingest`. Part 3: Memory
//! Systems, episode recording wired into `ingest`. Part 4 adds: opening
//! the ValueRegistry, a `values-status` command, a `record-outcome`
//! command, an admin-gated `lift-self-capability-ceiling` command, and —
//! real integration, not just parallel plumbing — every successful
//! `ingest` now also records a small Curiosity satisfaction outcome
//! (learning something new is literally prediction-error reduction
//! through learning, which is what Curiosity represents, Section 3).
//! Full REPL still arrives at Part 12.

use alynxis_core::core::admin::{AdminCredentialStore, AdminIdentity, AdminSession};
use alynxis_core::core::zones;
use alynxis_core::{logging, Config};
use alynxis_memory::MemoryStore;
use alynxis_values::{ValueKind, ValueRegistry};
use alynxis_worldmodel::WorldModel;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "alynxis", version, about = "Alynxis — Part 1: Foundation")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Override the config file path (default: ~/.alynxis/alynxis.toml)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the full Part 1 startup sequence and report status (default if no subcommand is given).
    Status,
    /// Set (or rotate) the admin credential (Argon2id, per Section 3c's
    /// resolution). Prompts for the secret interactively with echo
    /// disabled — never pass it as a CLI argument, since that would leak
    /// it into shell history.
    SetAdmin,
    /// Verify a candidate admin credential and report whether it authenticates.
    VerifyAdmin,
    /// Ingest a relational statement into the WorldModel (Section 7's
    /// concept-generalization path). Example: `alynxis ingest dog animal`
    /// or `alynxis ingest dog animal --relation is-a`.
    Ingest {
        subject: String,
        object: String,
        #[arg(long)]
        relation: Option<String>,
    },
    /// List every node currently sharing the given node's dominant
    /// category (Section 4's coarse index) — look the node up by label
    /// first with `ingest`'s output, or by exact label text here.
    SameCategory { label: String },
    /// List the most recent episodes recorded for the admin agent (or the
    /// self-node if `--self` is given) — Part 3's Memory Systems.
    RecentEpisodes {
        #[arg(long, default_value_t = 10)]
        limit: u32,
        #[arg(long = "self")]
        use_self_node: bool,
    },
    /// Show every seeded value's current baseline weight, floor, ceiling,
    /// and satisfaction EMA (Part 4).
    ValuesStatus,
    /// Record a satisfaction (positive) or frustration (negative) outcome
    /// against a value. `value` is one of: help, curiosity,
    /// social-connection, self-capability-enhancement, wellbeing-of-others.
    RecordOutcome {
        value: String,
        #[arg(allow_hyphen_values = true)]
        delta: f64,
    },
    /// Raise the self-capability-enhancement ceiling (Section 3f) — requires
    /// an authenticated admin session (prompts for the credential).
    LiftSelfCapabilityCeiling { new_ceiling: f64 },
}

fn main() {
    let cli = Cli::parse();

    let config_path = cli.config.clone().unwrap_or_else(default_config_path);

    let config = Config::load_or_init(&config_path).unwrap_or_else(|e| {
        eprintln!(
            "FATAL: failed to load/initialize config at {}: {e}",
            config_path.display()
        );
        std::process::exit(1);
    });

    if let Err(e) = config.ensure_data_dirs() {
        eprintln!(
            "FATAL: failed to create data directories under {}: {e}",
            config.data_dir.display()
        );
        std::process::exit(1);
    }

    let _logging_guards = logging::init_logging(&config.logs_dir(), &config.log_level);

    tracing::info!(
        "Alynxis starting — data_dir = {}",
        config.data_dir.display()
    );

    // Section 9: refuse to boot if Zone A has been tampered with.
    if let Err(e) = zones::verify_integrity() {
        tracing::error!("Zone A integrity check FAILED: {e}");
        eprintln!("FATAL: {e}");
        std::process::exit(1);
    }
    tracing::info!("Zone A integrity check passed.");

    let admin_identity_path = config.state_dir().join("admin_identity.json");
    let admin_identity = AdminIdentity::load_or_create(&admin_identity_path).unwrap_or_else(|e| {
        eprintln!("FATAL: failed to load/create admin identity: {e}");
        std::process::exit(1);
    });

    let admin_credential_path = config.state_dir().join("admin_credential.json");

    let worldmodel_path = config.state_dir().join("worldmodel.sqlite");
    let worldmodel = WorldModel::open(&worldmodel_path).unwrap_or_else(|e| {
        eprintln!("FATAL: failed to open WorldModel: {e}");
        std::process::exit(1);
    });
    let admin_agent_node = worldmodel
        .bind_admin_identity(admin_identity.id)
        .unwrap_or_else(|e| {
            eprintln!("FATAL: failed to bind admin identity to an agent-node: {e}");
            std::process::exit(1);
        });

    let memory_path = config.state_dir().join("memory.sqlite");
    let memory = MemoryStore::open(&memory_path).unwrap_or_else(|e| {
        eprintln!("FATAL: failed to open MemoryStore: {e}");
        std::process::exit(1);
    });

    let values_path = config.state_dir().join("values.json");
    let mut values = ValueRegistry::open(&values_path).unwrap_or_else(|e| {
        eprintln!("FATAL: failed to open ValueRegistry: {e}");
        std::process::exit(1);
    });

    match cli.command.unwrap_or(Command::Status) {
        Command::Status => {
            println!(
                "Alynxis — Part 1 (Foundation) + Part 2 (WorldModel) + Part 3 (Memory Systems) + Part 4 (Value System)"
            );
            println!("  config file:              {}", config_path.display());
            println!("  data dir:                 {}", config.data_dir.display());
            println!(
                "  require_zone_b_review:    {}",
                config.require_zone_b_review
            );
            println!(
                "  admin inactivity timeout: {}s",
                config.admin_inactivity_timeout_secs
            );
            println!(
                "  admin credential hashing: argon2id (Section 3c — single scheme, no alternative)"
            );
            println!("  Zone A integrity:         OK");
            println!("  admin identity:           {}", admin_identity.id);
            println!(
                "  admin credential configured: {}",
                AdminCredentialStore::is_configured(&admin_credential_path)
            );
            println!("  --- WorldModel (Part 2) ---");
            println!("  db path:                  {}", worldmodel_path.display());
            println!("  self-concept node:        {}", worldmodel.self_node_id());
            println!("  admin agent-node:         {admin_agent_node}");
            println!(
                "  nodes: {}   edges: {}",
                worldmodel.node_count().unwrap_or(0),
                worldmodel.edge_count().unwrap_or(0)
            );
            println!("  --- Memory Systems (Part 3) ---");
            println!("  db path:                  {}", memory_path.display());
            println!(
                "  episodes: {}   procedural patterns: {}",
                memory.episode_count().unwrap_or(0),
                memory.pattern_count().unwrap_or(0)
            );
            println!("  --- Value System (Part 4) ---");
            println!("  db path:                  {}", values_path.display());
            for kind in [
                ValueKind::Help,
                ValueKind::Curiosity,
                ValueKind::SocialConnection,
                ValueKind::SelfCapabilityEnhancement,
                ValueKind::WellbeingOfOthers,
            ] {
                if let Some(v) = values.get(kind) {
                    println!(
                        "  {kind:?}: weight={:.4} floor={:?} ceiling={:?} ema={:.4}",
                        v.baseline_weight, v.floor, v.ceiling, v.satisfaction_ema
                    );
                }
            }
        }
        Command::SetAdmin => {
            let secret = prompt_secret("New admin credential (long random string recommended): ");
            if secret.trim().is_empty() {
                eprintln!("Refusing to set an empty admin credential.");
                std::process::exit(1);
            }
            match AdminCredentialStore::set_credential(&secret, &admin_credential_path) {
                Ok(()) => println!("Admin credential set successfully (argon2id)."),
                Err(e) => {
                    eprintln!("Failed to set admin credential: {e}");
                    std::process::exit(1);
                }
            }
        }
        Command::VerifyAdmin => {
            let secret = prompt_secret("Admin credential: ");
            let inactivity_timeout = Duration::from_secs(config.admin_inactivity_timeout_secs);
            match AdminSession::authenticate(
                &secret,
                &admin_credential_path,
                admin_identity,
                inactivity_timeout,
            ) {
                Ok(session) => {
                    println!("Authenticated. Session valid: {}", session.is_valid());
                }
                Err(e) => {
                    println!("Authentication failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Command::Ingest {
            subject,
            object,
            relation,
        } => match worldmodel.ingest_relation(&subject, relation.as_deref(), &object) {
            Ok((subject_id, relation_id, object_id, edge_id)) => {
                println!("Ingested successfully.");
                println!("  subject  ({subject}): {subject_id}");
                if let Some(rid) = relation_id {
                    println!("  relation ({}): {rid}", relation.as_deref().unwrap_or(""));
                } else {
                    println!("  relation: none (untyped association)");
                }
                println!("  object   ({object}): {object_id}");
                println!("  edge:     {edge_id}");

                // Part 3 integration: every ingestion is something Alynxis
                // experienced, so it's naturally an episode — attributed
                // to the admin agent-node (who taught it), referencing
                // every WorldModel node/edge touched.
                let mut node_refs = vec![subject_id, object_id];
                if let Some(rid) = relation_id {
                    node_refs.push(rid);
                }
                match memory.record_episode(admin_agent_node, node_refs, vec![edge_id]) {
                    Ok(episode) => println!("  episode:  {} (Part 3)", episode.id),
                    Err(e) => eprintln!("  warning: failed to record episode: {e}"),
                }

                // Part 4 integration: learning something new is literally
                // prediction-error reduction through learning — exactly
                // what the Curiosity value represents (Section 3). A
                // small, fixed satisfaction nudge per ingestion; a richer
                // signal (e.g. scaled by how novel the ingestion actually
                // was) is a natural future refinement once System 1/2
                // exist to judge that.
                if let Err(e) = values.record_outcome(ValueKind::Curiosity, 0.3) {
                    eprintln!("  warning: failed to record curiosity outcome: {e}");
                }
            }
            Err(e) => {
                eprintln!("Ingestion failed: {e}");
                std::process::exit(1);
            }
        },
        Command::SameCategory { label } => {
            let seeds = worldmodel.seed_nodes_for_token(&label).unwrap_or_else(|e| {
                eprintln!("FATAL: lookup failed: {e}");
                std::process::exit(1);
            });
            let Some(&node_id) = seeds.first() else {
                println!("No node found with label {label:?}.");
                return;
            };
            match worldmodel.nodes_in_same_category(node_id) {
                Ok(siblings) if siblings.is_empty() => {
                    println!(
                        "No other nodes share {label:?}'s dominant category (or it has none yet)."
                    );
                }
                Ok(siblings) => {
                    println!("Nodes sharing {label:?}'s dominant category:");
                    for sibling_id in siblings {
                        let labels = worldmodel
                            .get_node(sibling_id)
                            .ok()
                            .flatten()
                            .map(|n| n.labels.join(", "))
                            .unwrap_or_default();
                        println!("  {sibling_id}  ({labels})");
                    }
                }
                Err(e) => {
                    eprintln!("Query failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Command::RecentEpisodes {
            limit,
            use_self_node,
        } => {
            let experiencer = if use_self_node {
                worldmodel.self_node_id()
            } else {
                admin_agent_node
            };
            match memory.recent_episodes(experiencer, limit) {
                Ok(episodes) if episodes.is_empty() => {
                    println!("No episodes recorded yet for this experiencer.");
                }
                Ok(episodes) => {
                    println!("Most recent {} episode(s):", episodes.len());
                    for ep in episodes {
                        println!(
                            "  {}  t={}ms  tier={:?}  nodes={}  edges={}",
                            ep.id,
                            ep.timestamp_ms,
                            ep.tier,
                            ep.node_refs.len(),
                            ep.edge_refs.len()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Query failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Command::ValuesStatus => {
            for kind in [
                ValueKind::Help,
                ValueKind::Curiosity,
                ValueKind::SocialConnection,
                ValueKind::SelfCapabilityEnhancement,
                ValueKind::WellbeingOfOthers,
            ] {
                if let Some(v) = values.get(kind) {
                    println!(
                        "{kind:?}\n  baseline_weight: {:.6}\n  floor:           {:?}\n  ceiling:         {:?}\n  satisfaction_ema:{:.6}\n",
                        v.baseline_weight, v.floor, v.ceiling, v.satisfaction_ema
                    );
                }
            }
        }
        Command::RecordOutcome { value, delta } => {
            let kind = match parse_value_kind(&value) {
                Some(k) => k,
                None => {
                    eprintln!(
                        "Unknown value {value:?}. Expected one of: help, curiosity, social-connection, self-capability-enhancement, wellbeing-of-others."
                    );
                    std::process::exit(1);
                }
            };
            match values.record_outcome(kind, delta) {
                Ok(()) => {
                    let v = values.get(kind).unwrap();
                    println!(
                        "Recorded. {kind:?} baseline_weight is now {:.6}.",
                        v.baseline_weight
                    );
                }
                Err(e) => {
                    eprintln!("Failed to record outcome: {e}");
                    std::process::exit(1);
                }
            }
        }
        Command::LiftSelfCapabilityCeiling { new_ceiling } => {
            let secret = prompt_secret("Admin credential (required to lift this ceiling): ");
            let inactivity_timeout = Duration::from_secs(config.admin_inactivity_timeout_secs);
            let session = match AdminSession::authenticate(
                &secret,
                &admin_credential_path,
                admin_identity,
                inactivity_timeout,
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Authentication failed: {e}");
                    std::process::exit(1);
                }
            };
            match values.lift_self_capability_ceiling(new_ceiling, &session) {
                Ok(()) => println!("Ceiling lifted to {new_ceiling}."),
                Err(e) => {
                    eprintln!("Failed to lift ceiling: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn parse_value_kind(s: &str) -> Option<ValueKind> {
    match s.to_ascii_lowercase().replace('_', "-").as_str() {
        "help" => Some(ValueKind::Help),
        "curiosity" => Some(ValueKind::Curiosity),
        "social-connection" => Some(ValueKind::SocialConnection),
        "self-capability-enhancement" => Some(ValueKind::SelfCapabilityEnhancement),
        "wellbeing-of-others" => Some(ValueKind::WellbeingOfOthers),
        _ => None,
    }
}

fn default_config_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".alynxis").join("alynxis.toml")
    } else {
        PathBuf::from("./alynxis_data/alynxis.toml")
    }
}

fn prompt_secret(prompt: &str) -> String {
    use std::io::IsTerminal;

    if std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt).unwrap_or_else(|e| {
            eprintln!("failed to read secret input: {e}");
            std::process::exit(1);
        })
    } else {
        // No controlling terminal (stdin piped/redirected — e.g. scripted
        // or automated invocation). rpassword needs a real TTY to disable
        // echo and hard-fails without one, so fall back to a plain-text
        // read here instead of crashing. There's nothing to hide echo of
        // in a non-interactive pipe anyway.
        use std::io::Write;
        eprint!("{prompt}");
        std::io::stderr().flush().ok();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap_or_else(|e| {
            eprintln!("failed to read secret input: {e}");
            std::process::exit(1);
        });
        input.trim_end_matches(['\r', '\n']).to_string()
    }
}
