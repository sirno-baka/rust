use crate::spec::{
    Arch, Cc, LinkerFlavor, Lld, Os, PanicStrategy, RelocModel, Target, TargetMetadata,
    TargetOptions,
};

pub(crate) fn target() -> Target {
    let mut options = TargetOptions {
        os: Os::Popugos,
        // i686 guarantees CMPXCHG8B, which lets LLVM implement Rust's 64-bit
        // atomics without relying on an external libatomic runtime.
        cpu: "i686".into(),
        features: "-mmx".into(),
        linker: Some("rust-lld".into()),
        linker_flavor: LinkerFlavor::Gnu(Cc::No, Lld::Yes),
        panic_strategy: PanicStrategy::Abort,
        relocation_model: RelocModel::Static,
        disable_redzone: true,
        singlethread: false,
        max_atomic_width: Some(64),
        ..Default::default()
    };
    options.add_pre_link_args(
        LinkerFlavor::Gnu(Cc::No, Lld::No),
        &["-melf_i386", "--entry=_start", "--undefined=_start"],
    );

    Target {
        llvm_target: "i386-unknown-none".into(),
        metadata: TargetMetadata {
            description: Some("32-bit PopugOS".into()),
            tier: None,
            host_tools: Some(false),
            std: Some(true),
        },
        pointer_width: 32,
        data_layout:
            "e-m:e-p:32:32-p270:32:32-p271:32:32-p272:64:64-i128:128-f64:32:64-f80:32-n8:16:32-S128"
                .into(),
        arch: Arch::X86,
        options,
    }
}
