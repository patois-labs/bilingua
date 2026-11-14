<?xml version="1.0" encoding="windows-1256"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>مرحبا بكم في المخبز</h1>
        <p>طلبك يحتوي على <b>وصل</b> <xsl:value-of select="count(item)"/> عناصر.</p>
        <img src="{@src}" alt="صورة خبز طازج"/>
        <xsl:if test="a"><xsl:text>حفظ وإغلاق</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
