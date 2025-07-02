//! Jepsen-style linearizability testing framework for Blixard
//! 
//! This framework provides comprehensive linearizability testing inspired by:
//! - Jepsen's Elle linearizability checker
//! - FoundationDB's simulation testing
//! - TigerBeetle's deterministic testing approach

pub mod history;
// TODO: Implement these modules
// pub mod checker;
// pub mod workload;
// pub mod failure_injection;
// pub mod specifications;
// pub mod analysis;

pub use history::{History, HistoryEntry, HistoryRecorder, Operation, Response};
// pub use checker::{LinearizabilityChecker, CheckResult, ViolationType};
// pub use workload::{WorkloadGenerator, WorkloadConfig, OperationMix};
// pub use failure_injection::{FailureInjector, FailureScenario};
// pub use specifications::{Specification, VmSpecification, KvSpecification, ClusterSpecification};
// pub use analysis::{HistoryAnalyzer, ViolationPattern, AnalysisReport};

/// Re-export commonly used types
pub mod prelude {
    pub use super::{
        History, HistoryEntry, HistoryRecorder, Operation, Response,
        // LinearizabilityChecker, CheckResult, ViolationType,
        // WorkloadGenerator, WorkloadConfig, OperationMix,
        // FailureInjector, FailureScenario,
        // Specification, VmSpecification, KvSpecification,
        // HistoryAnalyzer, ViolationPattern, AnalysisReport,
    };
}