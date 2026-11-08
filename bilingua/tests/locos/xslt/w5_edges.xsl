<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
<xsl:template match="/">
<td>Name</td>
<td><xsl:value-of select="a"/> left</td>
<td>Right <xsl:value-of select="b"/></td>
</xsl:template>
</xsl:stylesheet>
