# The oldest bash on a supported distro is 5.1 (RHEL 9, Ubuntu 22.04);
# 5.2 is Debian 13 / Ubuntu 24.04, 5.3 is current upstream.
variable "BASH_VERSION_MATRIX" {
    default = ["5.1", "5.2", "5.3"]
}

variable "FLYLINE_INSTALL_VERSION" {
    default = null
}

target "builder" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-builder"
}

target "built-artifact" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
}

# example command:
# docker buildx bake -f docker-bake.hcl extract-release-artifact
target "extract-release-artifact" {
    context = "."
    output = ["type=local,dest=docker/build"]
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
}

target "extract-integration-test-build-artifact" {
    context = "."
    output = ["type=local,dest=docker/build-integration-test"]
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
}

target "lib-tests" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-lib-tests"
}

target "specific-bash-version" {
    context = "."
    dockerfile = "docker/specific_bash_version.Dockerfile"
    name = "specific-bash-version-${replace(docker_bash_version, ".", "_")}"
    matrix = {
        docker_bash_version = BASH_VERSION_MATRIX
    }
    args = {
        DOCKER_BASH_VERSION = docker_bash_version
    }
    tags = ["bash-${docker_bash_version}"]
}

target "bash-integration-tests" {
    context = "."
    contexts = {
        built-artifact = "target:extract-integration-test-build-artifact",
        specific-bash-version = "target:specific-bash-version-${replace(docker_bash_version, ".", "_")}"
    }
    name = "bash-integration-test-${replace(docker_bash_version, ".", "_")}"
    matrix = {
        docker_bash_version = BASH_VERSION_MATRIX
    }
    dockerfile = "docker/bash_integration_test.Dockerfile"
    args = {
        DOCKER_BASH_VERSION = docker_bash_version
    }
}




# Runs `flyline --help` inside an interactive bash session, strips ANSI codes,
# and outputs flyline_help.txt to the project root.
target "extract-help-text" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact"
    }
    output = ["type=local,dest=./"]
    dockerfile = "docker/flyline_help.Dockerfile"
    target = "flyline-help-output"
}


target "demo-base" {
    context = "."
    dockerfile = "docker/demo_base.Dockerfile"
    contexts = {
        flyline-extracted-library = "target:built-artifact"
    }
}

target "_demo-base" {
    context = "."
    contexts = {
        demo-base = "target:demo-base"
    }
    output = ["type=local,dest=./"]
    # Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
    args = {
        BUILDKIT_SANDBOX_HOSTNAME = "my-hostname"
    }
}


target "demo-overview-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_overview.Dockerfile"
}

target "demo-prompts-ps1-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts_ps1.Dockerfile"
}

target "demo-prompts-rps1-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts_rps1.Dockerfile"
}

target "demo-prompts-ps1-fill-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts_ps1_fill.Dockerfile"
}

target "demo-prompts-ps2-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts_ps2.Dockerfile"
}

target "demo-prompts-final-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts_final.Dockerfile"
}

target "demo-prompts-prompt-ruler-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts_prompt_ruler.Dockerfile"
}

target "demo-fuzzy-suggestions-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_suggestions.Dockerfile"
}

target "demo-fuzzy-path-suggestions-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_path_suggestions.Dockerfile"
}

target "demo-custom-animation-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_custom_animation.Dockerfile"
}

target "demo-agent-mode-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_agent_mode.Dockerfile"
}

target "demo-ls-colors-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_ls_colors.Dockerfile"
}

target "demo-fuzzy-history-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_history.Dockerfile"
}

target "demo-inline-history-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_inline_history.Dockerfile"
}

target "demo-tab-completion-easing-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_tab_completion_easing.Dockerfile"
}

target "demo-auto-tab-completion-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_auto_tab_completion.Dockerfile"
}

target "demo-glob-expansion-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_glob_expansion.Dockerfile"
}

target "demo-flycomp-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_flycomp.Dockerfile"
}

target "demo-cursor-style-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_cursor_style.Dockerfile"
}

group "demos" {
    targets = [
        "demo-overview-extracted",
        "demo-prompts-ps1-extracted",
        "demo-prompts-rps1-extracted",
        "demo-prompts-ps1-fill-extracted",
        "demo-prompts-prompt-ruler-extracted",
        "demo-prompts-ps2-extracted",
        "demo-prompts-final-extracted",
        "demo-fuzzy-suggestions-extracted",
        "demo-fuzzy-path-suggestions-extracted",
        "demo-custom-animation-extracted",
        "demo-agent-mode-extracted",
        "demo-ls-colors-extracted",
        "demo-fuzzy-history-extracted",
        "demo-inline-history-extracted",
        "demo-tab-completion-easing-extracted",
        "demo-auto-tab-completion-extracted",
        "demo-glob-expansion-extracted",
        "demo-flycomp-extracted",
        "demo-cursor-style-extracted"
    ]
}

target "install-test-alpine" {
    context = "."
    dockerfile = "docker/install_test_alpine.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}

target "install-test-ubuntu" {
    context = "."
    dockerfile = "docker/install_test_ubuntu.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}

target "install-test-arch" {
    context = "."
    dockerfile = "docker/install_test_arch.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}

target "install-test-termux" {
    context = "."
    dockerfile = "docker/install_test_termux.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}


target "test-mimalloc-symbols" {
    context = "."
    dockerfile = "docker/arch_linux_mimalloc_symbols.Dockerfile"
}


