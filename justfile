# Default: Show help menu
default:
    @just help

# ============================================================================
# Help Command
# ============================================================================

help:
    @echo ""
    @echo "\033[1;36m======================================\033[0m"
    @echo "\033[1;36m    OpenCode RS SDK Commands          \033[0m"
    @echo "\033[1;36m======================================\033[0m"
    @echo ""
    @echo "\033[1;35m  Most Common Commands:\033[0m"
    @echo "  just \033[0;33mbuild\033[0m                    \033[0;32mBuild the crate\033[0m"
    @echo "  just \033[0;33mtest\033[0m                     \033[0;32mRun all tests\033[0m"
    @echo "  just \033[0;33mcheck\033[0m                    \033[0;32mCheck code compiles\033[0m"
    @echo ""
    @echo "\033[1;35m  Building:\033[0m"
    @echo "  just \033[0;33mbuild\033[0m                    \033[0;32mBuild development library\033[0m"
    @echo ""
    @echo "\033[1;35m  Verification:\033[0m"
    @echo "  just \033[0;33mcheck\033[0m                    \033[0;32mCheck code compiles\033[0m"
    @echo "  just \033[0;33mclippy\033[0m                   \033[0;32mRun clippy lints\033[0m"
    @echo "  just \033[0;33mfmt\033[0m                      \033[0;32mFormat code\033[0m"
    @echo "  just \033[0;33mfmt-check\033[0m                \033[0;32mCheck formatting\033[0m"
    @echo ""
    @echo "\033[1;35m  Testing:\033[0m"
    @echo "  just \033[0;33mtest\033[0m                     \033[0;32mRun all tests\033[0m"
    @echo "  just \033[0;33mtest-lib\033[0m                 \033[0;32mRun unit tests only\033[0m"
    @echo ""
    @echo "\033[1;35m  Examples:\033[0m"
    @echo "  just \033[0;33mexample\033[0m <name>          \033[0;32mRun a specific example\033[0m"
    @echo "  just \033[0;33moc-openagora\033[0m <prompt>    \033[0;32mRun OpenAgora example\033[0m"
    @echo "  just \033[0;33mendpoint-parity\033[0m          \033[0;32mCompare endpoints with OpenAPI\033[0m"
    @echo ""
    @echo "\033[1;35m  Utilities:\033[0m"
    @echo "  just \033[0;33mclean\033[0m                    \033[0;32mClean build artifacts\033[0m"
    @echo ""
    @echo ""

# ============================================================================
# Building Commands
# ============================================================================
import 'justfiles/building/build.just'

# ============================================================================
# Verification Commands
# ============================================================================
import 'justfiles/verification/check.just'
import 'justfiles/verification/clippy.just'
import 'justfiles/verification/fmt.just'
import 'justfiles/verification/fmt-check.just'

# ============================================================================
# Testing Commands
# ============================================================================
import 'justfiles/testing/test.just'
import 'justfiles/testing/test-lib.just'

# ============================================================================
# Example Commands
# ============================================================================
import 'justfiles/examples/run-example.just'
import 'justfiles/examples/oc-openagora.just'
import 'justfiles/examples/endpoint-parity.just'

# ============================================================================
# Utilities Commands
# ============================================================================
import 'justfiles/utilities/clean.just'
