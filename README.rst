=====
Djade
=====

.. image:: https://img.shields.io/github/actions/workflow/status/adamchainz/djade/main.yml.svg?branch=main&style=for-the-badge
   :target: https://github.com/adamchainz/djade/actions?workflow=CI

.. image:: https://img.shields.io/pypi/v/djade.svg?style=for-the-badge
   :target: https://pypi.org/project/djade/

.. image:: https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit&logoColor=white&style=for-the-badge
   :target: https://github.com/pre-commit/pre-commit
   :alt: pre-commit

.. figure:: https://raw.githubusercontent.com/adamchainz/djade/main/logo.svg
   :alt: You can have any colour you like, as long as it’s jade.

..

A Django template formatter.

    You can have any colour you like, as long as it’s [d]jade.

Djade formats Django template syntax with a style based on the `template style guide <https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/coding-style/#template-style>`__ in Django’s documentation.
It does not format HTML or other templated languages.

Djade is fast because it iss built in Rust: benchmarked taking 20ms to format 377 templates.

Read more in `the introductory post <https://adamj.eu/tech/2024/09/26/django-introducing-djade/>`__, or below.

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

You can also install Djade as a `pre-commit <https://pre-commit.com/>`__ hook.

**First,** add the following to the ``repos`` section of your ``.pre-commit-config.yaml`` file (`docs <https://pre-commit.com/#plugins>`__):

.. code-block:: yaml

    -   repo: https://github.com/adamchainz/djade-pre-commit
        rev: ""  # Replace with the latest tag on GitHub
        hooks:
        -   id: djade
            args: [--target-version, "5.1"]  # Replace with Django version

The separate repository enables installation without compiling the Rust code.

The default configuration uses pre-commit’s |files option|__ to pick up all text files in directories called ``templates`` (`source <https://github.com/adamchainz/djade-pre-commit/blob/main/.pre-commit-hooks.yaml>`__).
You may wish to override this if you have templates in different directories by adding ``files`` to the hook configuration in your ``.pre-commit-config.yaml`` file.

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

Keep the hook installed to continue formatting your templates.
pre-commit’s ``autoupdate`` command will upgrade Djade so you can take advantage of future features.

Usage
=====

``djade`` is a command line tool that rewrites files in place.
Pass a list of template files to format them:

.. code-block:: console

    $ djade --target-version 5.1 templates/**/*.html
    Rewriting templates/locomotives/steam_engine.html
    Rewriting templates/locomotives/diesel.html

Djade can also upgrade some old template syntax.
Add the ``--target-version`` option with your Django version as ``<major>.<minor>`` to enable applicable fixers:

.. code-block:: console

    $ djade --target-version 5.1 templates/**/*.html
    Rewriting templates/locomotives/steam_engine.html

Djade does not have any ability to recurse through directories.
Use the pre-commit integration, globbing, or another technique to apply it to many files.
For example, |with git ls-files pipe xargs|_:

.. |with git ls-files pipe xargs| replace:: with ``git ls-files | xargs``
.. _with git ls-files pipe xargs: https://adamj.eu/tech/2022/03/09/how-to-run-a-command-on-many-files-in-your-git-repository/

.. code-block:: sh

    git ls-files -z -- '*.html' | xargs -0 djade

…or PowerShell’s |ForEach-Object|__:

.. |ForEach-Object| replace:: ``ForEach-Object``
__ https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/foreach-object

.. code-block:: powershell

    git ls-files -- '*.html' | %{djade $_}

Options
=======

``--target-version``
--------------------

Optional: the version of Django to target, in the format ``<major>.<minor>``.
If provided, Djade enables its fixers for versions up to and including the target version.
See the list of available versions with ``djade  --help``.

``--check``
-----------

Avoid writing any formatted files back.
Instead, exit with a non-zero status code if any files would have been modified, and zero otherwise.

Formatting
==========

Djade aims to format Django template syntax in a consistent, clean way.
It wants to be like `Black <https://black.readthedocs.io/en/stable/>`__: opinionated and free of configuration.
Djade’s style is based on the rules listed in the Django contribution style guide’s `template style section <https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/coding-style/#template-style>`__, plus some more.

Djade does not aim to format the host language of templates (HTML, etc.).
That is a much broader scope and hard to do without semantic changes.
For example, whitespace is significant in some HTML contexts, such as in ``<pre>`` tags, so even adjusting indentation can affect the meaning.

Below are the rules that Djade implements.

Rules from the Django style guide:

* Single spaces at the start and end of variables and tags:

  .. code-block:: diff

    -{{train}}
    +{{ train }}

    -{%  blow whistle  %}
    +{% blow whistle %}

* Label ``{% endblock %}`` tags that aren’t on the same line as their opening ``{% block %}`` tag:

  .. code-block:: diff

     {% block funnel %}
     ...
    -{% endblock %}
    +{% endblock funnel %}

* Sort libraries in ``{% load %}`` tags:

  .. code-block:: diff

    -{% load coal boiler %}
    +{% load boiler coal %}

* Inside variables, no spaces around filters:

  .. code-block:: diff

    -{{ fire | stoke }}
    +{{ fire|stoke }}

* Inside tags, single spaces between tokens:

  .. code-block:: diff

    -{% if  locomotive  ==  'steam engine'  %}
    +{% if locomotive == 'steam engine' %}

* Unindent top-level ``{% block %}`` and ``{% endblock %}`` tags when ``{% extends %}`` is used:

  .. code-block:: diff

    -  {% extends 'engine.html' %}
    +{% extends 'engine.html' %}

    -  {% block boiler %}
    +{% block boiler %}
       ...
    -  {% endblock boiler %}
    +{% endblock boiler %}

Extra rules:

* No leading empty lines:

  .. code-block:: diff

    -
     {% extends 'engine.html' %}
     ...

* No trailing empty lines:

  .. code-block:: diff

     ...
     {% endblock wheels %}
    -
    -

* Single spaces at the start and end of comments:

  .. code-block:: diff

    -{#choo choo#}
    +{# choo choo #}

* No labels in ``{% endblock %}`` tags on the same line as their opening ``{% block %}`` tag:

  .. code-block:: diff

    -{% block funnel %}...{% endblock funnel %}
    +{% block funnel %}...{% endblock %}

* Merge consecutive ``{% load %}`` tags:

  .. code-block:: diff

    -{% load boiler %}
    -
    -{% load coal %}
    +{% load boiler coal %}

* Sort loaded items in ``{% load ... from .. %}`` tags:

.. code-block:: diff

    -{% load steam heat from boiler %}
    +{% load heat steam from boiler %}

* Unindent ``{% extends %}`` tags:

  .. code-block:: diff

    -  {% extends 'engine.html' %}
    +{% extends 'engine.html' %}

* Exactly one blank line between top-level ``{% block %}`` and ``{% endblock %}`` tags when ``{% extends %}`` is used:

.. code-block:: diff

     {% extends 'engine.html' %}

    -
     {% block funnel %}
       ...
     {% endblock funnel %}
    +
     {% block boiler %}
       ...
     {% endblock boiler %}

Fixers
======

Djade applies the below fixes based on the target Django version from ``--target-version``.

Django 4.2+: ``length_is`` -> ``length``
----------------------------------------

From the `release note <https://docs.djangoproject.com/en/4.2/releases/4.2/#id1>`__:

    The ``length_is`` template filter is deprecated in favor of ``length`` and the ``==`` operator within an ``{% if %}`` tag.

Djade updates usage of the deprecated filter within ``if`` tags, without other conditions, appropriately:

.. code-block:: diff

    -{% if engines|length_is:1 %}
    +{% if engines|length == 1 %}

Django 4.1+: empty ID ``json_script`` fixer
-------------------------------------------

From the `release note <https://docs.djangoproject.com/en/4.1/releases/4.1/#templates>`__:

    The HTML ``<script>`` element ``id`` attribute is no longer required when wrapping the ``json_script`` template filter.

Djade removes the argument where ``json_script`` is passed an empty string, to avoid emitting ``id=""``:

.. code-block:: diff

    -{% tracks|json_script:"" %}
    +{% tracks|json_script %}

Django 3.1+: ``trans`` -> ``translate``, ``blocktrans`` / ``endblocktrans`` -> ``blocktranslate`` / ``endblocktranslate``
-------------------------------------------------------------------------------------------------------------------------

From the `release note <https://docs.djangoproject.com/en/3.1/releases/3.1/#templates>`__:

    The renamed ``translate`` and ``blocktranslate`` template tags are introduced for internationalization in template code.
    The older ``trans`` and ``blocktrans`` template tags aliases continue to work, and will be retained for the foreseeable future.

Djade updates the deprecated tags appropriately:

.. code-block:: diff

    -{% load blocktrans trans from i18n %}
    +{% load blocktranslate translate from i18n %}

    -{% trans "Engine colours" %}
    +{% translate "Engine colours" %}

    -{% blocktrans with colour=engine.colour %}
    +{% blocktranslate with colour=engine.colour %}
     This engine is {{ colour }}.
    -{% endblocktrans %}
    +{% endblocktranslate %}

Django 3.1+: ``ifequal`` and ``ifnotequal`` -> ``if``
-----------------------------------------------------

From the `release note <https://docs.djangoproject.com/en/3.1/releases/3.1/#id2:~:text=The%20%7B%25%20ifequal%20%25%7D%20and%20%7B%25%20ifnotequal%20%25%7D%20template%20tags>`__:

    The ``{% ifequal %}`` and ``{% ifnotequal %}`` template tags are deprecated in favor of ``{% if %}``.

Djade updates the deprecated tags appropriately:

.. code-block:: diff

    -{% ifequal engine.colour 'blue' %}
    +{% if engine.colour == 'blue' %}
     Thomas!
    -{% endifequal %}
    +{% endif %}

    -{% ifnotequal engine.colour 'blue' %}
    +{% if engine.colour != 'blue' %}
     Not Thomas.
    -{% endifnotequal %}
    +{% endif %}

Django 2.1+: ``admin_static`` and ``staticfiles`` -> ``static``
---------------------------------------------------------------

From the `release note <https://docs.djangoproject.com/en/2.1/releases/2.1/#features-deprecated-in-2-1>`__:

    ``{% load staticfiles %}`` and ``{% load admin_static %}`` are deprecated in favor of ``{% load static %}``, which works the same.

Djade updates ``{% load %}`` tags appropriately:

.. code-block:: diff

    -{% load staticfiles %}
    +{% load static %}

    -{% load admin_static %}
    +{% load static %}
