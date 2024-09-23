=========
Changelog
=========

* Add formatting of filters in variables:

  .. code-block:: diff

      -{{ egg | crack }}
      +{{ egg|crack }}

* Add unindenting of ``{% extends %}`` tags, and top-level ``{% block %}`` and ``{% endblock %}`` tags where ``{% extends %}`` is used.

  `PR #30 <https://github.com/adamchainz/djade/pull/30>`__.

* Add ``--target-version`` option to specify target Django version.

* Add Django 2.1+ fixer to replace ``{% load %}`` of ``admin_static`` and ``staticfiles`` with ``static``.

  `PR #34 <https://github.com/adamchainz/djade/pull/34>`__.

0.1.0 (2024-09-21)
------------------

* First release on PyPI.
