//! Headless driver for the aircraft design core: validation and evaluation of
//! documents without any UI. This is the Milestone verification harness used
//! by CI and the fixture suite.

use aircraft_graph::{build, check_predicates, evaluate_graph, FieldKind};
use aircraft_model::aircraft::Component;
use aircraft_model::{parse_document, Diagnostic};
use clap::{Parser, Subcommand};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "aircraft",
    about = "Validate and evaluate aircraft definition documents",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a document: schema, semantics, expressions, and predicates.
    Validate { path: PathBuf },
    /// Validate, then print the evaluated station table in document units.
    Evaluate { path: PathBuf },
    /// Validate, then generate geometry and print the derived report.
    Report { path: PathBuf },
}

/// Exit codes: 0 valid, 1 error diagnostics, 2 usage or I/O failure.
pub const EXIT_VALID: i32 = 0;
pub const EXIT_DIAGNOSTICS: i32 = 1;
pub const EXIT_FAILURE: i32 = 2;

/// Run the CLI with the given arguments (without the program name) and
/// return the exit code and everything that would be printed.
pub fn run<I, T>(args: I) -> (i32, String)
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let mut output = String::new();
    let cli = match Cli::try_parse_from(
        std::iter::once("aircraft".to_string()).chain(args.into_iter().map(Into::into)),
    ) {
        Ok(cli) => cli,
        Err(error) => return (EXIT_FAILURE, format!("{error}\n")),
    };

    let (path, command) = match cli.command {
        Command::Validate { path } => (path, CommandKind::Validate),
        Command::Evaluate { path } => (path, CommandKind::Evaluate),
        Command::Report { path } => (path, CommandKind::Report),
    };
    let code = run_command(&path, command, &mut output);
    (code, output)
}

fn run_command(path: &Path, command: CommandKind, out: &mut String) -> i32 {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            let _ = writeln!(out, "cannot read {}: {error}", path.display());
            return EXIT_FAILURE;
        }
    };

    let (doc, mut diagnostics) = match parse_document(&source) {
        Ok(parsed) => parsed,
        Err(errors) => {
            print_diagnostics(out, &errors);
            let _ = writeln!(out, "result: invalid");
            return EXIT_DIAGNOSTICS;
        }
    };

    let (graph, warnings) = match build(&doc) {
        Ok(build) => build,
        Err(errors) => {
            print_diagnostics(out, &errors);
            print_summary(out, &doc, &diagnostics);
            let _ = writeln!(out, "result: invalid");
            return EXIT_DIAGNOSTICS;
        }
    };
    diagnostics.extend(warnings);

    let evaluation = evaluate_graph(&graph, &doc);
    diagnostics.extend(evaluation.diagnostics.clone());
    diagnostics.extend(check_predicates(&graph, &doc, &evaluation));

    if let Some(error) = diagnostics.iter().find(|d| d.is_error()) {
        let _ = writeln!(out, "stopping before {}: {error}", command.description());
        print_summary(out, &doc, &diagnostics);
        let _ = writeln!(out, "result: invalid");
        return EXIT_DIAGNOSTICS;
    }

    match command {
        CommandKind::Validate => {}
        CommandKind::Evaluate => print_stations(out, &doc, &graph, &evaluation.values),
        CommandKind::Report => {
            let opened = aircraft_engine::Engine::open(doc.clone());
            let report = opened
                .map_err(|errors| errors.into_iter().next().expect("non-empty"))
                .and_then(|mut engine| {
                    engine
                        .report(&aircraft_engine::CancellationToken::default())
                        .map_err(|error| match error {
                            aircraft_engine::MeshJobError::Failed(diagnostics) => {
                                diagnostics.into_iter().next().expect("non-empty")
                            }
                            other => {
                                let _ = writeln!(out, "mesh job failed: {other:?}");
                                aircraft_model::Diagnostic::error(
                                    aircraft_model::Code::MeshFailure,
                                    "mesh job failed",
                                )
                            }
                        })
                });
            match report {
                Ok(report) => print_report(out, &report),
                Err(diagnostic) => {
                    let _ = writeln!(out, "{diagnostic}");
                    let _ = writeln!(out, "result: invalid");
                    return EXIT_DIAGNOSTICS;
                }
            }
        }
    }
    print_summary(out, &doc, &diagnostics);
    print_diagnostics(out, &diagnostics);

    let valid = !diagnostics.iter().any(Diagnostic::is_error);
    let _ = writeln!(out, "result: {}", if valid { "valid" } else { "invalid" });
    if valid {
        EXIT_VALID
    } else {
        EXIT_DIAGNOSTICS
    }
}

#[derive(Debug, Clone, Copy)]
enum CommandKind {
    Validate,
    Evaluate,
    Report,
}

impl CommandKind {
    fn description(self) -> &'static str {
        match self {
            CommandKind::Validate => "validation",
            CommandKind::Evaluate => "evaluation",
            CommandKind::Report => "report",
        }
    }
}

fn print_report(out: &mut String, report: &aircraft_engine::report::AircraftReport) {
    for wing in &report.wings {
        let planform = &wing.planform;
        let _ = writeln!(out, "wing {} planform (half / full):", wing.wing_id);
        let _ = writeln!(
            out,
            "  reference area: {:.4} / {:.4} m^2",
            planform.reference_area.half, planform.reference_area.full
        );
        let _ = writeln!(
            out,
            "  span:           {:.4} / {:.4} m",
            planform.span.half, planform.span.full
        );
        let _ = writeln!(
            out,
            "  MAC:            {:.4} m at x={:.4} z={:.4}",
            planform.mac, planform.mac_le[0], planform.mac_le[2]
        );
        let _ = writeln!(out, "  aspect ratio:   {:.4}", planform.aspect_ratio);
        if let Some(taper) = planform.taper_ratio {
            let _ = writeln!(out, "  taper ratio:    {:.4}", taper);
        }
        for (panel, index) in planform.panels.iter().zip(1..) {
            let deg = |radian: f64| radian * 180.0 / std::f64::consts::PI;
            let _ = writeln!(
                out,
                "  panel {}: LE sweep {:7.3} deg, quarter-chord {:7.3} deg, TE {:7.3} deg, dihedral {:7.3} deg",
                index,
                deg(panel.leading_edge_sweep),
                deg(panel.quarter_chord_sweep),
                deg(panel.trailing_edge_sweep),
                deg(panel.dihedral)
            );
        }
        if let Some(volume) = &wing.volume {
            let _ = writeln!(
                out,
                "  volume:         {:.5} / {:.5} m^3, wetted area {:.4} / {:.4} m^2",
                volume.volume.half,
                volume.volume.full,
                volume.wetted_area.half,
                volume.wetted_area.full
            );
        }
        let _ = writeln!(
            out,
            "  interactive mesh: {} vertices, {} triangles",
            wing.mesh_statistics.vertices, wing.mesh_statistics.triangles
        );
    }
}

fn print_summary(
    out: &mut String,
    doc: &aircraft_model::AircraftDefinition,
    diagnostics: &[Diagnostic],
) {
    let wings = doc
        .components
        .iter()
        .filter(|component| matches!(component, Component::Wing(_)))
        .count();
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    let warnings = diagnostics.len() - errors;
    let _ = writeln!(
        out,
        "{} {:?}: {} wing(s), {} parameter(s), {} error(s), {} warning(s)",
        doc.id,
        doc.name,
        wings,
        doc.parameters.len(),
        errors,
        warnings
    );
}

fn print_diagnostics(out: &mut String, diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        let _ = writeln!(out, "diagnostics: none");
        return;
    }
    let _ = writeln!(out, "diagnostics:");
    for diagnostic in diagnostics {
        let _ = writeln!(out, "  {diagnostic}");
    }
}

fn print_stations(
    out: &mut String,
    doc: &aircraft_model::AircraftDefinition,
    graph: &aircraft_graph::Graph,
    values: &[f64],
) {
    let _ = writeln!(
        out,
        "evaluated stations (document units: {}, {}):",
        doc.units.length_unit_name(),
        doc.units.angle_unit_name()
    );
    for (component_index, component) in doc.components.iter().enumerate() {
        let Component::Wing(wing) = component;
        let _ = writeln!(out, "  wing {}", wing.id);
        for (station_index, station) in wing.stations.iter().enumerate() {
            let field = |kind: FieldKind| {
                graph
                    .station_field_node(component_index, station_index, kind)
                    .map(|node| values[node])
            };
            // Canonical meters/radians back into document units.
            let format_length = |value: Option<f64>| {
                value
                    .map(|v| doc.units.length_from_canonical(v))
                    .unwrap_or(f64::NAN)
            };
            let format_angle = |value: Option<f64>| {
                value
                    .map(|v| doc.units.angle_from_canonical(v))
                    .unwrap_or(f64::NAN)
            };
            let _ = writeln!(
                out,
                "    {:<10} x={:>9.4}  y={:>9.4}  z={:>9.4}  chord={:>8.4}  twist={:>7.3}",
                station.id,
                format_length(field(FieldKind::PositionX)),
                format_length(field(FieldKind::PositionY)),
                format_length(field(FieldKind::PositionZ)),
                format_length(field(FieldKind::Chord)),
                format_angle(field(FieldKind::Twist)),
            );
        }
    }
}
