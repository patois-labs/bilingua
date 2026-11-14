=============
Project Guide
=============

Introduction
============

This is a *simple* paragraph with ``code`` and a :ref:`label <x>` role,
spanning two lines with |sub| and a link_ and `Docs <https://example.com>`_.

.. note:: Remember to save your work.

   A second note paragraph.

.. warning::
   :class: big

   Do not press the red button [1]_.

.. code-block:: python

   print("hello")

Example::

    literal text
    not translated

- First item
- Second item with **bold** text

  Nested paragraph in item.

1. Numbered one
#. Numbered two

Term
   Definition of the term.

.. _link: https://example.com
.. |sub| replace:: substituted
.. [1] The footnote text.
.. include:: other.rst
.. This is a comment
   spanning lines.

:Author: Jane Doe
:Version: 1.0

>>> print("doctest")
doctest

| Line block one
| Line block two

.. list-table:: Fruit table
   :header-rows: 1

   * - Name
     - Colour

.. figure:: img.png
   :alt: alt text

   The figure caption.

.. versionadded:: 2.1
   The new option.
