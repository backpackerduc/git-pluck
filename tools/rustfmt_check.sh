#!/usr/bin/env sh
# pre-commit hook: rustfmt --check on staged .rs files; on failure show how to fix.
rustfmt --check "$@"
status=$?
if [ "$status" -ne 0 ]; then
  echo ""
  echo "rustfmt found formatting issues in the staged files (diff above)."
  echo "To auto-fix them, run:"
  echo "    rustfmt $*"
  echo "Consider auto-formatting in your code editor!"
fi
exit "$status"
