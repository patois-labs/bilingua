<?xml version="1.0" encoding="EUC-JP"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>ベーカリーへようこそ</h1>
        <p>ご注文は <b>届きました</b> <xsl:value-of select="count(item)"/> 点.</p>
        <img src="{@src}" alt="焼きたてパンの写真"/>
        <xsl:if test="a"><xsl:text>保存して閉じる</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
