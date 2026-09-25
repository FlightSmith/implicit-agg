//! Headless driver for the aircraft design core: validation, evaluation, and
//! geometry export without any UI. This is the Milestone verification harness
//! used by CI and the fixture suite, and the batch entry point for exports.

use aircraft_graph::{build, check_predicates, evaluate_graph, FieldKind};
use aircraft_model::aircraft::Component;
use aircraft_model::{parse_document, Diagnostic};
use clap::{Parser, Subcommand};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "aircraft",
    about = "Validate, evaluate, and export aircraft definition documents",
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
    /// Validate, then write the wing's geometry: STEP analytic solids or
    /// STL/OBJ/GLB meshes.
    Export {
        path: PathBuf,
        /// Output file; the format defaults to the file extension.
        output: PathBuf,
        /// Output format override (step, stl, obj, glb).
        #[arg(short, long)]
        format: Option<FormatArg>,
        /// Write the mirrored full model instead of the source half.
        #[arg(long)]
        full: bool,
        /// Mesh tessellation tier (ignored for STEP).
        #[arg(short, long, default_value = "export")]
        quality: QualityArg,
        /// Wing component index.
        #[arg(long, default_value_t = 0)]
        component: usize,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum FormatArg {
    Step,
    Stl,
    Obj,
    Glb,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum QualityArg {
    Interactive,
    Settled,
    Export,
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
        Command::Export {
            path,
            output,
            format,
            full,
            quality,
            component,
        } => {
            let format = match format {
                Some(format) => format,
                None => match FormatArg::from_extension(&output) {
                    Some(format) => format,
                    None => {
                        return (
                            EXIT_FAILURE,
                            format!(
                                "cannot infer a format from {}: use --format step|stl|obj|glb\n",
                                output.display()
                            ),
                        );
                    }
                },
            };
            (
                path,
                CommandKind::Export(ExportOptions {
                    output,
                    format,
                    full,
                    quality,
                    component,
                }),
            )
        }
    };
    let code = run_command(&path, command, &mut output);
    (code, output)
}

/// Everything the export subcommand needs beyond the document path.
struct ExportOptions {
    output: PathBuf,
    format: FormatArg,
    full: bool,
    quality: QualityArg,
    component: usize,
}

/// Write the wing's geometry in the requested format. Returns the exit code;
/// on success the caller still prints the usual summary tail.
fn export_wing(
    out: &mut String,
    doc: &aircraft_model::AircraftDefinition,
    options: &ExportOptions,
) -> i32 {
    let mut engine = match aircraft_engine::Engine::open(doc.clone()) {
        Ok(engine) => engine,
        Err(errors) => {
            print_diagnostics(out, &errors);
            return EXIT_DIAGNOSTICS;
        }
    };
    let token = aircraft_engine::CancellationToken::default();
    let name = options
        .output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("wing");
    let bytes = match options.format {
        FormatArg::Step => engine
            .export_step(options.component, options.full, &token)
            .map(|text| text.into_bytes()),
        FormatArg::Stl => engine.export(
            options.component,
            options.quality.into(),
            options.full,
            meshio::MeshFormat::StlBinary,
            name,
            &token,
        ),
        FormatArg::Obj => engine.export(
            options.component,
            options.quality.into(),
            options.full,
            meshio::MeshFormat::Obj,
            name,
            &token,
        ),
        FormatArg::Glb => engine.export(
            options.component,
            options.quality.into(),
            options.full,
            meshio::MeshFormat::Glb,
            name,
            &token,
        ),
    };
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(aircraft_engine::MeshJobError::Failed(diagnostics)) => {
            print_diagnostics(out, &diagnostics);
            return EXIT_DIAGNOSTICS;
        }
        Err(error) => {
            let _ = writeln!(out, "export failed: {error:?}");
            return EXIT_FAILURE;
        }
    };
    match std::fs::write(&options.output, bytes) {
        Ok(()) => {
            let _ = writeln!(out, "wrote {}", options.output.display());
            EXIT_VALID
        }
        Err(error) => {
            let _ = writeln!(out, "cannot write {}: {error}", options.output.display());
            EXIT_FAILURE
        }
    }
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
        CommandKind::Export(options) => {
            let code = export_wing(out, &doc, &options);
            if code != EXIT_VALID {
                let _ = writeln!(out, "result: invalid");
                return code;
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

enum CommandKind {
    Validate,
    Evaluate,
    Report,
    Export(ExportOptions),
}

impl CommandKind {
    fn description(self) -> &'static str {
        match self {
            CommandKind::Validate => "validation",
            CommandKind::Evaluate => "evaluation",
            CommandKind::Report => "report",
            CommandKind::Export(_) => "export",
        }
    }
}

impl FormatArg {
    fn from_extension(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "step" | "stp" => Some(Self::Step),
            "stl" => Some(Self::Stl),
            "obj" => Some(Self::Obj),
            "glb" | "gltf" => Some(Self::Glb),
            _ => None,
        }
    }
}

impl From<QualityArg> for aircraft_engine::MeshQuality {
    fn from(quality: QualityArg) -> Self {
        match quality {
            QualityArg::Interactive => Self::Interactive,
            // The auto-settle tier mirror of the wasm binding: finer than the
            // live preview, cheaper than a full export run.
            QualityArg::Settled => Self::Export(aircraft_engine::ExportTolerances {
                max_chordal_deviation: Some(2.5e-3),
                max_edge_length: Some(0.6),
                max_normal_angle_deg: Some(10.0),
            }),
            QualityArg::Export => Self::Export(Default::default()),
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
