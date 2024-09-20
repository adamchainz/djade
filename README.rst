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

.. figure:: logo.svg
   :alt: Any color you like, as long as it’s jade.

..

Django template formatter.

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

For example:

.. code-block:: sh

    djade templates/index.html
