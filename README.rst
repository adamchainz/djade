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

Based on Django’s `template style guide <https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/coding-style/#template-style>`__.
Very fast because it’s built in Rust: benchmarked taking 20ms to format 377 templates.

----

**Improve your Django and Git skills** with `my books <https://adamj.eu/books/>`__.

----

Installation
============

Use **pip**:

.. code-block:: sh

    python -m pip install djade

Python 3.8 to 3.13 supported.

pre-commit hook
---------------

You can also install django-upgrade as a `pre-commit <https://pre-commit.com/>`__ hook.

**First,** add the following to the ``repos`` section of your ``.pre-commit-config.yaml`` file (`docs <https://pre-commit.com/#plugins>`__):

.. code-block:: yaml

    -   repo: https://github.com/adamchainz/djade-pre-commit
        rev: ""  # replace with latest tag on GitHub
        hooks:
        -   id: djade

The separate repository is used to enable installation without compiling the Rust code.

The default configuration uses pre-commit’s |files option|__ to pick up on any file in a directory called ``templates`` (`source <https://github.com/adamchainz/djade-pre-commit/blob/main/.pre-commit-hooks.yaml>`__).
You may wish to override this if you have templates in different directories, by adding ``files`` to the hook configuration in your ``.pre-commit-config.yaml`` file.

.. |files option| replace:: ``files`` option
__ https://pre-commit.com/#creating-new-hooks

**Second,** format your entire project:

.. code-block:: sh

    pre-commit run djade --all-files

Check these changes for any potential Djade bugs and commit them.
Try ``git diff --ignore-all-space`` to check non-whitespace changes.

**Third,** consider adding the previous commit SHA to a |.git-blame-ignore-revs file|__.
This will prevent the initial formatting commit from showing up in ``git blame``.

.. |.git-blame-ignore-revs file| replace:: ``.git-blame-ignore-revs`` file
__ https://docs.github.com/en/repositories/working-with-files/using-files/viewing-a-file#ignore-commits-in-the-blame-view

Keep the hook installed in order to continue formatting your templates.
pre-commit’s ``autoupdate`` command will upgrade Djade so you can take advantage of future features.

Usage
=====

``djade`` is a commandline tool that rewrites files in place.
Run it on one or more files to modify them if necessary:

.. code-block:: console

    $ djade templates/index.html
    Rewriting templates/index.html

Djade does not have any ability to recurse through directories.
Use the pre-commit integration, globbing, or another technique for applying to many files.
For example, |with git ls-files pipe xargs|_:

.. |with git ls-files pipe xargs| replace:: with ``git ls-files | xargs``
.. _with git ls-files pipe xargs: https://adamj.eu/tech/2022/03/09/how-to-run-a-command-on-many-files-in-your-git-repository/

.. code-block:: sh

    git ls-files -z -- '*.py' | xargs -0 djade

…or PowerShell’s |ForEach-Object|__:

.. |ForEach-Object| replace:: ``ForEach-Object``
__ https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/foreach-object

.. code-block:: powershell

    git ls-files -- '*.py' | %{djade $_}

Rules
=====

Djade implements some rules listed in the Django contribution style guide’s `template style section <https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/coding-style/#template-style>`__:

* One space around variables and tags:

  .. code-block:: diff

      -{{egg}}
      +{{ egg }}

      -{%  crack egg  %}
      +{% crack egg %}

* Label ``{% endblock %}`` tags that aren’t on the same line as their opening ``{% block %}`` tag:

  .. code-block:: diff

       {% block shell %}
       ...
      -{% endblock %}
      +{% endblock shell %}

* Sort libraries in ``{% load %}`` tags:

  .. code-block:: diff

      -{% load omelette frittata %}
      +{% load friattata omelette %}

* Unindent top-level ``{% block %}`` and ``{% endblock %}`` tags when ``{% extends %}`` is used:

  .. code-block:: diff

      -  {% extends 'egg.html' %}
      +{% extends 'egg.html' %}

      -  {% block yolk %}
      +{% block yolk %}
           ...
      -  {% endblock yolk %}
      +{% endblock yolk %}

Djade also implements some extra rules:

* No leading empty lines:

  .. code-block:: diff

      -
       {% extends 'white.html' %}
       ...

* No trailing empty lines:

  .. code-block:: diff

       ...
       {% endblock content %}
      -
      -

* One space around comment tags:

  .. code-block:: diff

    -{#egg#}
    +{# egg #}

* No labels in ``{% endblock %}`` tags on the same line as their opening ``{% block %}`` tag:

  .. code-block:: diff

      -{% block shell %}...{% endblock shell %}
      +{% block shell %}...{% endblock %}

* Merge consecutive ``{% load %}`` tags:

  .. code-block:: diff

      -{% load omelette %}
      -
      -{% load frittata %}
      +{% load frittata omelette %}


* Unindent ``{% extends %}`` tags:

  .. code-block:: diff

      -  {% extends 'egg.html' %}
      +{% extends 'egg.html' %}
