=========
Changelog
=========

1.0.0 (2024-09-25)
------------------

* Add formatting of variables:

  .. code-block:: diff

      -{{ egg | crack }}
      +{{ egg|crack }}

* Add formatting of block tags:

  .. code-block:: diff

      -{% if  breakfast  ==  'scrambled eggs'  %}
      +{% if breakfast == 'scrambled eggs' %}

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
