# Internationalization consumer

The framework exposes translation lookup and refresh seams; the application owns catalogs, namespaces, locale selection, and catalog transport. Hosts can install a built-in catalog and update translated values without embedding application strings in framework crates.

Missing keys remain identifiable to the host and view. Catalog replacement should be validated before use so a malformed update does not discard the last working catalog.
