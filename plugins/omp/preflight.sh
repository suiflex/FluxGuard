#!/bin/sh
set -eu

operation=${FLUXGUARD_OPERATION:-inspect_targeted}
importance=${FLUXGUARD_IMPORTANCE:-optional}

exec fluxguard hook "$operation" --importance "$importance" --json
