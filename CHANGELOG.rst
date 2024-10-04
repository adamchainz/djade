=========
Changelog
=========

1.2.0 (2024-10-04)
------------------

* Add ``--check`` option to report if changes would be made.

 Thanks to Coen van der Kamp for the request in `Issue #79 <https://github.com/adamchainz/djade/issues/79>`__.

* Improve output.
  Individual file names are no longer reported, except with ``--check``.
  A final summary message lists the number of reformatted and already-formatted files.

1.1.1 (2024-09-29)
------------------

* Detect and use the file’s newline style (Unix ``\n`` versus Windows ``\r\n``).

  Thanks to George Kussumoto in `PR #73 <https://github.com/adamchainz/djade/pull/73>`__.

1.1.0 (2024-09-27)
------------------

* Fix handling of the ``{% load ... from .. %}`` syntax, and add sorting of loaded items:

  .. code-block:: diff

    -{% load steam heat from boiler %}
    +{% load heat steam from boiler %}

  Thanks to Eric Holscher for the report in `Issue #62 <https://github.com/adamchainz/djade/issues/62>`__.

* Update ``{% load ... from i18n %}`` tags in the Django 3.1+ translate tag fixer.

  `Issue #67 <https://github.com/adamchainz/djade/issues/67>`__.

* Fix crash with unlabelled opening ``{% block %}`` tags.

  `PR #63 <https://github.com/adamchainz/djade/pull/63>`__.

1.0.0 (2024-09-25)
------------------

* Add formatting of variables:

  .. code-block:: diff

      -{{ fire | stoke }}
      +{{ fire|stoke }}

* Add formatting of block tags:

  .. code-block:: diff

      -{% if  engine.colour  ==  'blue'  %}
      +{% if engine.colour == 'blue' %}

* Add unindenting of ``{% extends %}`` tags, and top-level ``{% block %}`` and ``{% endblock %}`` tags where ``{% extends %}`` is used.

  `PR #30 <https://github.com/adamchainz/djade/pull/30>`__.

* Add spacing adjustment of top-level ``{% block %}`` and ``{% endblock %}`` tags where ``{% extends %}`` is used.

  `PR #55 <https://github.com/adamchainz/djade/pull/55>`__.

* Add ``--target-version`` option to specify target Django version.

* Add Django 4.2+ fixer to migrate ``{% if %}`` with ``length_is`` to use ``length`` and ```==``.

  `PR #54 <https://github.com/adamchainz/djade/pull/54>`__.

* Add Django 4.1 fixer to migrate use of the ``json_script`` filter with an empty string to drop the argument.

  `PR #56 <https://github.com/adamchainz/djade/pull/56>`__.

* Add Django 3.1+ fixer to migrate ``{% trans %}`` to ``{% translate %}`` and ``{% blocktrans %}`` / ``{% endblocktrans %}`` to ``{% blocktranslate %}`` / ``{% endblocktranslate %}``.

  `PR #53 <https://github.com/adamchainz/djade/pull/53>`__.

* Add Django 3.1+ fixer to migrate ``{% ifequal %}`` / ``{% endifequal %}`` and ``{% ifnotequal %}`` / ``{% endifnotequal %}`` to ``{% if %}`` / ``{% endif %}``.

  `PR #35 <https://github.com/adamchainz/djade/pull/35>`__.

* Add Django 2.1+ fixer to replace ``{% load %}`` of ``admin_static`` and ``staticfiles`` with ``static``.

  `PR #34 <https://github.com/adamchainz/djade/pull/34>`__.

0.1.0 (2024-09-21)
------------------

* First release on PyPI.
