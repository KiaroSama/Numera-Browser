//! Native selection of compiler-generated browser programs. Application logic
//! lives in browser-page-rust; these wrappers only initialize and call exports.

const FACTORY: &str = include_str!("../../browser-page-rust/generated/runtime.js");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Identity,
    Trust,
    Dns,
    Font,
    Ip,
    Tls,
    WebRtc,
    LocationHost,
    InternalLinks,
    InspectIdentity,
    HostEnvironment,
    SmokeValue,
}

impl Program {
    pub const fn export(self) -> &'static str {
        match self {
            Self::Identity => "apply_identity",
            Self::Trust => "trust_probe",
            Self::Dns => "dns_probe",
            Self::Font => "font_probe",
            Self::Ip => "ip_probe",
            Self::Tls => "tls_probe",
            Self::WebRtc => "webrtc_probe",
            Self::LocationHost => "location_host",
            Self::InternalLinks => "internal_links",
            Self::InspectIdentity => "inspect_identity",
            Self::HostEnvironment => "host_environment",
            Self::SmokeValue => "smoke_value",
        }
    }
}

/// Classic function-expression artifact, with no runtime code compilation.
pub fn artifact() -> &'static str {
    FACTORY
}

/// SDK evaluate accepts a one-argument function and awaits its return value.
pub fn expression(program: Program) -> String {
    format!("(argument) => ({FACTORY})().{}(argument)", program.export())
}

/// The caller supplies JSON emitted by SourceJson, retaining original spacing,
/// number representation and ASCII escapes before the first page script runs.
pub(crate) fn identity(encoded_config: &str) -> String {
    format!("(() => {{\nconst CFG = {encoded_config};\n({FACTORY})().apply_identity(CFG);\n}})();")
}
