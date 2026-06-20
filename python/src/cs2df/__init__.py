"""cs2df — reference exporter & validator for cs2-demo-format v3.

Keep this module import-light: `cs2df validate` must work without the native
demoparser2 / pandas stack installed. Heavy imports are deferred into the
submodules that need them.
"""

__version__ = "3.1.0"

SCHEMA_VERSION = "cs2-demo-format/3.0"
EXPORTER_NAME = "cs2df"
