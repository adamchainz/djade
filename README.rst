=====
djade
=====

.. image:: https://img.shields.io/github/actions/workflow/status/adamchainz/djade/main.yml.svg?branch=main&style=for-the-badge
   :target: https://github.com/adamchainz/djade/actions?workflow=CI

.. image:: https://img.shields.io/badge/Coverage-100%25-success?style=for-the-badge
   :target: https://github.com/adamchainz/djade/actions?workflow=CI

.. image:: https://img.shields.io/pypi/v/djade.svg?style=for-the-badge
   :target: https://pypi.org/project/djade/

.. image:: https://img.shields.io/badge/code%20style-black-000000.svg?style=for-the-badge
   :target: https://github.com/psf/black

.. image:: https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white&style=for-the-badge
   :target: https://github.com/pre-commit/pre-commit
   :alt: pre-commit

.. figure:: https://raw.githubusercontent.com/adamchainz/djade/main/logo.svg
   :alt: Any color you like, as long as it’s jade.

..

A Django template formatter.

----

**Improve your Django and Git skills** with `my books <https://adamj.eu/books/>`__.

----

Installation
============

Use **pip**:

.. code-block:: sh

    python -m pip install djade

Python 3.8 to 3.13 supported.

Usage
=====

``djade`` is a commandline tool that rewrites files in place.
Run it on a file to modify it:

.. code-block:: sh

    djade templates/index.html

Djade implements some rules listed in the Django contribution style guide’s `template style section <https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/coding-style/#template-style>`__:

* One space around variables and tags:

  .. code-block:: diff

      -{{egg}}
      +{{ egg }}

      -{%  crack egg  %}
      +{% crack egg %}

* Add labels to ``{% endblock %}`` tags that aren’t on the same line as their opening ``{% block %}`` tag:

  .. code-block:: diff

       {% block shell %}
       ...
      -{% endblock %}
      +{% endblock shell %}

* In ``{% load %}`` tags, alphabetically sort libraries:

  .. code-block:: diff

      -{% load omelette frittata %}
      +{% load friattata omelette %}

Djade also implements some extra rules:

* Leading empty lines are removed:

  .. code-block:: diff

      -
       {% extends 'white.html' %}
       ...

* Trailing empty lines are removed, and a trailing newline inserted if necessary:

  .. code-block:: diff

       ...
       {% endblock content %}
      -
      -

* One space around comment tags:

  .. code-block:: diff

    -{#egg#}
    +{# egg #}

* Remove labels from ``{% endblock %}`` tags on the same line as their opening ``{% block %}`` tag:

  .. code-block:: diff

      -{% block shell %}...{% endblock shell %}
      +{% block shell %}...{% endblock %}

* Merge consecutive ``{% load %}`` tags:

  .. code-block:: diff

      -{% load omelette %}
      -
      -{% load frittata %}
      +{% load frittata omelette %}
