<t:stylesheet version="2.0" xmlns:t="http://www.w3.org/1999/XSL/Transform"
    xmlns:xsl="urn:not-xslt">
  <t:template match="/">
    <t:text disable-output-escaping="yes">&lt;br/&gt;</t:text>
    <b>Bold</b> <i>Italic</i>
    <p>Read <a href="{@u}" title="Open the guide">the guide</a> now.</p>
    <t:variable name="label"><span>Saved label</span></t:variable>
    <xsl:note>Not an instruction</xsl:note>
  </t:template>
</t:stylesheet>
